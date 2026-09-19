/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_plugin.h>
#include <libheif/heif_items.h>
#include <libheif/heif_metadata.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
static uint32_t v[12];
static unsigned char *packet;
static int poll, allocations, current_class;
static size_t packet_at;
static const heif_encoder_parameter *parameters[4] = {NULL};
static heif_error ok(void) { return (heif_error){0,0,"Success"}; }
static void error(heif_error e) { printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL"); }
static const char *name(void) { return "independent encoder probe"; }
static heif_error allocate(void **p) { allocations++;printf(" alloc");if(allocations==2 && (v[10]&16384)){*p=NULL;return (heif_error){8,0,"alpha allocation"};}*p=malloc(1);return ok(); }
static heif_error get_int(void *p,int *v) { (void)p;*v=23;printf(" get23");return ok(); }
static heif_error set_int(void *p,int v) { (void)p;printf(" set%d",v);return ok(); }
static heif_error get_named(void *p,const char *name,int *v) { (void)p;printf(" get-%s",name);*v=31;return ok(); }
static heif_error set_named(void *p,const char *name,int v) { (void)p;printf(" set-%s-%d",name,v);return ok(); }
static heif_error get_string(void *p,const char *name,char *v,int n) { (void)p;printf(" get-%s-%d",name,n);snprintf(v,n,"copied string");return ok(); }
static heif_error set_string(void *p,const char *name,const char *v) { (void)p;printf(" set-%s-%s",name,v);return ok(); }
static void release(void *p) { printf(" free");free(p); }
static const heif_encoder_parameter **list(void *p) { (void)p;return parameters; }
static void query(heif_colorspace *cs,heif_chroma *ch) {
 printf(" query%d,%d",*cs,*ch);
 if(v[5]!=99) { *cs=(heif_colorspace)v[5];*ch=(heif_chroma)v[6]; }
}
static void query2(void *p,heif_colorspace *cs,heif_chroma *ch) { (void)p;printf(" v2");query(cs,ch); }
static void size(void *p,uint32_t w,uint32_t h,uint32_t *ow,uint32_t *oh) {
 (void)p;printf(" size%u,%u",w,h);*ow=w+(int32_t)v[7];*oh=h+(int32_t)v[7];
}
static heif_error encode(void *p,const heif_image *image,heif_image_input_class cls) {
 (void)p;poll=0;packet_at=0;current_class=cls;printf(" encode%d,%d,%d",cls,heif_image_get_colorspace(image),heif_image_get_chroma_format(image));
 for(int ch=0;ch<=10;ch++) if(heif_image_has_channel(image,(heif_channel)ch)) {
  int stride=0,w=heif_image_get_width(image,(heif_channel)ch),h=heif_image_get_height(image,(heif_channel)ch),b=heif_image_get_bits_per_pixel(image,(heif_channel)ch);
  const unsigned char *data=heif_image_get_plane_readonly(image,(heif_channel)ch,&stride);uint32_t hash=0;
  for(int y=0;y<h;y++)for(int x=0;x<w*((b+7)/8);x++)hash=hash*33+data[y*stride+x];
  printf(" plane%d,%d,%d,%d,%u",ch,w,h,b,hash);
 }
 heif_color_profile_nclx *n=NULL;heif_error e=heif_image_get_nclx_color_profile(image,&n);printf(" color%d",e.code);
 if(n){printf(",%d,%d,%d,%d",n->color_primaries,n->transfer_characteristics,n->matrix_coefficients,n->full_range_flag);heif_nclx_color_profile_free(n);}
 return (v[8]==1 || (cls==2 && (v[10]&32768)))?(heif_error){8,0,"encode callback"}:ok();
}
static heif_error data(void *p,uint8_t **out,int *n,heif_encoded_data_type *type) {
 (void)p;printf(" data%d,%d",poll,type!=NULL);
 if((v[8]==2 || (current_class==2 && (v[10]&65536))) && poll==1)return (heif_error){8,2006,"Usage error: Invalid parameter value: data callback"};
 if(v[10]&262144) {
  if(packet_at+4<=v[11] && v[9]) { uint32_t length;memcpy(&length,packet+packet_at,4);packet_at+=4;if(length>v[11]-packet_at)return (heif_error){8,0,"invalid probe packet"};*out=packet+packet_at;*n=length;packet_at+=length; } else { *out=NULL;*n=0; }
 } else if(poll<(int)v[9]) { *out=packet;*n=v[11]; } else { *out=NULL;*n=0; }
 poll++;return ok();
}
static heif_error write_data(heif_context *ctx,const void *bytes,size_t n,void *u) {
 (void)ctx;(void)u;printf(" file%zu:",n);for(size_t i=0;i<n;i++)printf("%02x",((const uint8_t*)bytes)[i]);return ok();
}
#define END(t,f) (offsetof(t,f)+sizeof(((t*)0)->f))
int main(void) {
 setvbuf(stdout,NULL,_IONBF,0);
 while(fread(v,sizeof(v),1,stdin)==1) {
  if(v[11]>1000000)return 2;packet=malloc(v[11]+1);if(fread(packet,1,v[11],stdin)!=v[11])return 2;
  for(int i=0;i<3;i++){heif_encoder_parameter p={0};p.version=v[0]>=3?2:1;p.name=i==0?"integer":i==1?"boolean":"string";p.type=(heif_encoder_parameter_type)(i+1);size_t n=p.version==1?offsetof(heif_encoder_parameter,has_default):sizeof(p);void *copy=malloc(n);memcpy(copy,&p,n);parameters[i]=copy;}
  allocations=0;heif_encoder_plugin plugin={0};plugin.plugin_api_version=v[0];plugin.compression_format=(v[10]&262144)?heif_compression_HEVC:heif_compression_AV1;plugin.id_name="encode-probe";plugin.priority=100000;plugin.supports_lossy_compression=1;plugin.get_plugin_name=name;plugin.new_encoder=allocate;plugin.free_encoder=release;plugin.list_parameters=list;plugin.query_input_colorspace=query;plugin.query_input_colorspace2=query2;plugin.encode_image=encode;plugin.get_compressed_data=data;plugin.query_encoded_size=size;plugin.get_parameter_quality=get_int;plugin.set_parameter_quality=set_int;plugin.get_parameter_lossless=get_int;plugin.set_parameter_lossless=set_int;plugin.get_parameter_logging_level=get_int;plugin.set_parameter_logging_level=set_int;plugin.get_parameter_integer=get_named;plugin.set_parameter_integer=set_named;plugin.get_parameter_boolean=get_named;plugin.set_parameter_boolean=set_named;plugin.get_parameter_string=get_string;plugin.set_parameter_string=set_string;
  size_t length=v[0]<=1?END(heif_encoder_plugin,get_compressed_data):v[0]==2?END(heif_encoder_plugin,query_input_colorspace2):v[0]==3?END(heif_encoder_plugin,query_encoded_size):sizeof(plugin);
  heif_encoder_plugin *old=malloc(length);memcpy(old,&plugin,length);error(heif_register_encoder_plugin(old));
  heif_context *ctx=heif_context_alloc();heif_context_set_write_mini_format(ctx,v[10]&64);heif_encoder *encoder=NULL;error(heif_context_get_encoder_for_format(ctx,plugin.compression_format,&encoder));
  heif_image *image=NULL;error(heif_image_create(7,5,(heif_colorspace)v[1],(heif_chroma)v[2],&image));
  for(int ch=0;ch<=10;ch++) {
   int present=v[1]==2?ch==0:v[1]==0?ch<3:v[2]==3?(ch>=3&&ch<=5):ch==10;
   if(ch==6 && v[2]!=11 && v[2]!=13 && v[2]!=15 && (v[10]&4096))present=1;if(!present)continue;int w=7,h=5;if(v[1]==0&&ch>0&&ch<3){if(v[2]==1||v[2]==2)w=4;if(v[2]==1)h=3;}
   error(heif_image_add_plane(image,(heif_channel)ch,w,h,v[3]));int stride=0;unsigned char *plane=heif_image_get_plane(image,(heif_channel)ch,&stride);int bytes=(v[3]>8?2:1)*(ch==10?((v[2]==11||v[2]==13||v[2]==15)?4:3):1);
   for(int y=0;y<h;y++)for(int x=0;x<w*bytes;x++)plane[y*stride+x]=(x*7+y*13+ch*29)&255;
  }
  heif_encoding_options *options=heif_encoding_options_alloc();options->image_orientation=v[4];options->save_alpha_channel=(v[10]&4096)!=0;
  if(v[10]&8192)heif_image_set_premultiplied_alpha(image,1);
  if(v[10]&1024){heif_content_light_level clli={123,45};heif_image_set_content_light_level(image,&clli);heif_mastering_display_colour_volume mdcv={{1,2,3},{4,5,6},7,8,999,100};heif_image_set_mastering_display_colour_volume(image,&mdcv);heif_ambient_viewing_environment amve={500,100,200};heif_image_set_ambient_viewing_environment(image,&amve);heif_image_set_nominal_diffuse_white_luminance(image,333);}
  heif_color_profile_nclx *n=heif_nclx_color_profile_alloc();n->color_primaries=9;n->transfer_characteristics=16;n->matrix_coefficients=9;n->full_range_flag=1;
  if(v[10]&1)error(heif_image_set_nclx_color_profile(image,n));
  if(v[10]&2)options->output_nclx_profile=n;
  if(v[10]&4)error(heif_image_set_raw_color_profile(image,"prof",(v[10]&2048)?(void*)packet:(void*)"profile",(v[10]&2048)?v[11]:7));
  options->save_two_colr_boxes_when_ICC_and_nclx_available=(v[10]&8)!=0;
  options->macOS_compatibility_workaround_no_nclx_profile=(v[10]&16)!=0;
  heif_image_handle *handle=(void*)(uintptr_t)1;heif_error result=heif_context_encode_image(ctx,image,encoder,(v[10]&32)?NULL:options,&handle);error(result);printf(" output%d,%d",handle!=NULL,handle==(void*)(uintptr_t)1);
  if(result.code==0 && handle) { printf(" dims%d,%d,%d,%d",heif_image_handle_get_width(handle),heif_image_handle_get_height(handle),heif_image_handle_get_ispe_width(handle),heif_image_handle_get_ispe_height(handle));if(v[10]&128)error(heif_context_add_exif_metadata(ctx,handle,"II*\0\10\0\0\0\0\0",10));if(v[10]&256)error(heif_context_add_XMP_metadata2(ctx,handle,packet,v[11],(heif_metadata_compression)((v[10]&512)?3:0)));if(v[10]&131072){heif_image_handle *thumb=NULL;error(heif_context_encode_thumbnail(ctx,image,handle,encoder,options,4,&thumb));printf(" thumb%d",thumb!=NULL);if(thumb)heif_image_handle_release(thumb);}heif_image_handle_release(handle); }
  heif_writer writer={1,write_data};error(heif_context_write(ctx,&writer,NULL));error(heif_context_write(ctx,&writer,NULL));
  heif_image_release(image);heif_nclx_color_profile_free(n);heif_encoding_options_free(options);heif_encoder_release(encoder);heif_context_free(ctx);heif_deinit();free(old);for(int i=0;i<3;i++)free((void*)parameters[i]);free(packet);puts("");
 }
 return ferror(stdin)?2:0;
}
