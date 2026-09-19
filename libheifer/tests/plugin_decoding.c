/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_plugin.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static uint32_t v[12];
static unsigned polls;
static heif_error ok(void){return (heif_error){0,0,"callback-success"};}
static heif_error failure(unsigned stage){return v[2]==stage?(heif_error){7,(int)v[3],v[4]?"Decoder plugin generated an error: Unspecified: detail":"detail"}:ok();}
static void err(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static const char* name(void){return "callback decoder";}
static int support(heif_compression_format f){return f==(int)v[1]?777:0;}
static void init(void){printf(" init");}
static void cleanup(void){printf(" cleanup");}
static heif_error allocate(void** out){printf(" old-new");*out=malloc(1);polls=0;return failure(1);}
static heif_error allocate2(void** out,const heif_decoder_plugin_options* o){printf(" new%d,%d,%d,%llu,%d",o->format,o->strict_decoding,o->num_threads,(unsigned long long)o->limits->max_image_size_pixels,o->limits->parent!=NULL);*out=malloc(1);polls=0;return failure(1);}
static void release(void* p){printf(" free%d",p!=NULL);free(p);}
static void strict(void* p,int x){(void)p;printf(" strict%d",x);}
static heif_error push(void* p,const void* data,size_t size){(void)p;printf(" push%zu:",size);for(size_t i=0;i<size;i++)printf("%02x",((const uint8_t*)data)[i]);return failure(2);}
static heif_error push2(void* p,const void* data,size_t size,uintptr_t user){printf(" user%zu",(size_t)user);return push(p,data,size);}
static heif_error flush(void* p){(void)p;printf(" flush");return failure(3);}
static heif_error decode(void* p,heif_image** out){(void)p;printf(" decode%u",polls++);if(v[2]==4)return failure(4);if(polls<=v[5])return ok();int width=(int)v[6],height=(int)v[7];heif_error e=heif_image_create(width,height,heif_colorspace_monochrome,heif_chroma_monochrome,out);if(e.code)return e;e=heif_image_add_plane(*out,heif_channel_Y,width,height,(int)v[8]);if(e.code)abort();int stride;uint8_t* bytes=heif_image_get_plane(*out,heif_channel_Y,&stride);for(int y=0;y<height;y++)for(int x=0;x<width*((v[8]+7)/8);x++)bytes[y*stride+x]=(uint8_t)(y*53+x*31+19);return ok();}
static heif_error decode1(void* p,heif_image** out,const heif_security_limits* l){printf(" limits%llu",(unsigned long long)l->max_image_size_pixels);return decode(p,out);}
static heif_error decode2(void* p,heif_image** out,uintptr_t* user,const heif_security_limits* l){printf(" out-user%d",user==NULL);return decode1(p,out,l);}
#define END(t,f) (offsetof(t,f)+sizeof(((t*)0)->f))
int main(void){uint32_t n;while(fread(v,sizeof(v),1,stdin)==1&&fread(&n,4,1,stdin)==1){uint8_t* data=malloc(n?n:1);if(fread(data,1,n,stdin)!=n)return 2;heif_init(NULL);heif_decoder_plugin p={0};p.plugin_api_version=(int)v[0];p.get_plugin_name=name;p.init_plugin=init;p.deinit_plugin=cleanup;p.does_support_format=support;p.new_decoder=(v[9]&1)?NULL:allocate;p.free_decoder=release;p.push_data=push;p.decode_image=decode;p.set_strict_decoding=strict;p.id_name="callback-decoder";p.decode_next_image=(v[9]&2)?NULL:decode1;p.new_decoder2=allocate2;p.push_data2=(v[9]&4)?NULL:push2;p.flush_data=flush;p.decode_next_image2=(v[9]&8)?NULL:decode2;size_t sz=v[0]==1?END(heif_decoder_plugin,decode_image):v[0]==2?END(heif_decoder_plugin,set_strict_decoding):v[0]==3?END(heif_decoder_plugin,id_name):v[0]==4?END(heif_decoder_plugin,decode_next_image):sizeof(p);void* record=malloc(sz);memcpy(record,&p,sz);err(heif_register_decoder_plugin(record));heif_context* ctx=heif_context_alloc();heif_context_set_max_decoding_threads(ctx,0);err(heif_context_read_from_memory_without_copy(ctx,data,n,NULL));heif_image_handle* handle=NULL;heif_error e=heif_context_get_primary_image_handle(ctx,&handle);err(e);if(!e.code){heif_colorspace cs=heif_colorspace_undefined;heif_chroma ch=heif_chroma_undefined;err(heif_image_handle_get_preferred_decoding_colorspace(handle,&cs,&ch));printf(" handle%d,%d,%d,%d",cs,ch,heif_image_handle_get_luma_bits_per_pixel(handle),heif_image_handle_get_chroma_bits_per_pixel(handle));heif_decoding_options* opts=heif_decoding_options_alloc();opts->strict_decoding=(uint8_t)v[10];opts->num_codec_threads=(int32_t)v[11];if(v[9]&16)opts->decoder_id="callback-decoder";if(v[9]&32)opts->decoder_id="absent";heif_context_free(ctx);ctx=NULL;for(unsigned j=0;j<(v[0]>=5?2u:1u);j++){heif_image* image=(heif_image*)(uintptr_t)1;e=heif_decode_image(handle,&image,heif_colorspace_undefined,heif_chroma_undefined,opts);err(e);printf(" null%d",image==NULL);if(!e.code){printf(" image%d,%d,%d,%d",heif_image_get_primary_width(image),heif_image_get_primary_height(image),heif_image_get_colorspace(image),heif_image_get_chroma_format(image));for(int ch=0;ch<=10;ch++){if(!heif_image_has_channel(image,(heif_channel)ch))continue;int stride;const uint8_t* bytes=heif_image_get_plane_readonly(image,(heif_channel)ch,&stride);int w=heif_image_get_width(image,(heif_channel)ch),h=heif_image_get_height(image,(heif_channel)ch),bits=heif_image_get_bits_per_pixel(image,(heif_channel)ch);printf(" plane%d,%d,%d,%d:",ch,w,h,bits);for(int y=0;y<h;y++)for(int x=0;x<w*((bits+7)/8);x++)printf("%02x",bytes[y*stride+x]);}heif_image_release(image);}opts->decoder_id="absent";}heif_decoding_options_free(opts);heif_image_handle_release(handle);}if(ctx)heif_context_free(ctx);heif_deinit();free(record);free(data);puts("");}return 0;}
