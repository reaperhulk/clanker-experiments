/* SPDX-License-Identifier: LGPL-3.0-or-later
 * Original-header client: area extraction and physical/visible extension.
 * Native NULL image/out pointers, shrinking-width writes and oversized OOM
 * requests are undefined/unbounded and intentionally outside this corpus. */
#define main transforms_main
#include "transforms.c"
#undef main
#include <libheif/heif_components.h>
#include <libheif/heif_properties.h>
#include <libheif/heif_tai_timestamps.h>
#include <inttypes.h>
static FILE* pixels;
static uint64_t emit(const uint8_t* p,size_t n){uint64_t h=1469598103934665603ULL;if(fwrite(p,1,n,pixels)!=n)abort();for(size_t k=0;k<n;k++)h=(h^p[k])*1099511628211ULL;return h;}
static uint32_t chh(uint32_t h,int c,int ch){return (c==1||c==2)&&ch==1?h/2+(h&1):h;}
static void metadata(heif_image* image){
 heif_tai_timestamp_packet t={.version=1,.tai_timestamp=UINT64_C(0xfedcba9876543210),.synchronization_state=201,.timestamp_generation_failure=128,.timestamp_is_modified=7};
 error(heif_image_set_tai_timestamp(image,&t));
 heif_content_light_level cl={123,456};heif_image_set_content_light_level(image,&cl);
 heif_ambient_viewing_environment am={12345,321,654};heif_image_set_ambient_viewing_environment(image,&am);
 heif_mastering_display_colour_volume md={{1,2,3},{4,5,6},7,8,999,10};heif_image_set_mastering_display_colour_volume(image,&md);
 heif_image_set_nominal_diffuse_white_luminance(image,654321);
 heif_image_set_premultiplied_alpha(image,1);heif_image_set_pixel_aspect_ratio(image,17,19);
 heif_image_add_decoding_warning(image,(heif_error){heif_error_Invalid_input,heif_suberror_End_of_data,"ignored"});
}
static void snapshot(heif_image* im,uint32_t probe_h){
 int ch=heif_image_get_chroma_format(im);uint32_t a,b;heif_image_get_pixel_aspect_ratio(im,&a,&b);
 printf("I%d,%d,%d,%d,%d,%u,%u;",heif_image_get_primary_width(im),heif_image_get_primary_height(im),heif_image_get_colorspace(im),ch,heif_image_is_premultiplied_alpha(im),a,b);
 heif_tai_timestamp_packet* t=NULL;error(heif_image_get_tai_timestamp(im,&t));if(t){printf("T%u,%" PRIu64 ",%u,%u,%u;",t->version,t->tai_timestamp,t->synchronization_state,t->timestamp_generation_failure,t->timestamp_is_modified);heif_tai_timestamp_packet_release(t);}
 heif_content_light_level cl={0};heif_image_get_content_light_level(im,&cl);printf("C%d,%u,%u;",heif_image_has_content_light_level(im),cl.max_content_light_level,cl.max_pic_average_light_level);
 heif_ambient_viewing_environment am={0};printf("A%d,",heif_image_get_ambient_viewing_environment(im,&am));printf("%u,%u,%u;",am.ambient_illumination,am.ambient_light_x,am.ambient_light_y);
 heif_mastering_display_colour_volume md={0};heif_image_get_mastering_display_colour_volume(im,&md);printf("M%d,%u,%u,%u,%u;",heif_image_has_mastering_display_colour_volume(im),md.display_primaries_x[2],md.display_primaries_y[1],md.max_display_mastering_luminance,md.min_display_mastering_luminance);
 printf("N%d,%u;",heif_image_has_nominal_diffuse_white_luminance(im),heif_image_get_nominal_diffuse_white_luminance(im));
 heif_error warnings[8];int nw=heif_image_get_decoding_warnings(im,0,warnings,8);printf("W%d;",nw);for(int j=0;j<nw;j++)error(warnings[j]);
 heif_color_profile_nclx* n=NULL;error(heif_image_get_nclx_color_profile(im,&n));if(n){printf("n%d,%d,%d,%d;",n->color_primaries,n->transfer_characteristics,n->matrix_coefficients,n->full_range_flag);heif_nclx_color_profile_free(n);}
 size_t raw=heif_image_get_raw_color_profile_size(im);printf("r%u,%zu;",heif_image_get_color_profile_type(im),raw);if(raw){uint8_t* p=malloc(raw);error(heif_image_get_raw_color_profile(im,p));printf("%" PRIx64 ";",emit(p,raw));free(p);}
 for(int k=0;k<=14;k++){
  int c=k==14?65535:k;if(!heif_image_has_channel(im,c))continue;
  int w=heif_image_get_width(im,c),h=heif_image_get_height(im,c),bits=heif_image_get_bits_per_pixel(im,c);size_t stride=0;const uint8_t* p=heif_image_get_plane_readonly2(im,c,&stride);
  printf("P%d,%d,%d,%d,%d,%zu;",c,w,h,bits,heif_image_get_bits_per_pixel_range(im,c),stride);
  /* Inspect hidden padding as well as every visible row without reading slack. */
  uint32_t mh=h>0?((h+1)&~1u):64;if(mh<64)mh=64;uint32_t rows=chh(probe_h,c,ch);if(rows>mh)rows=mh;if(h>0&&rows<(uint32_t)h)rows=h;
  for(uint32_t y=0;y<rows;y++)printf("%" PRIx64 ",",emit(p+y*stride,stride));
 }
 uint32_t ids[40],nids=heif_image_get_number_of_used_components(im);if(nids>40)abort();heif_image_get_used_component_ids(im,ids);printf("D%u;",nids);
 for(uint32_t k=0;k<nids;k++){
  uint32_t id=ids[k],w=heif_image_get_component_width(im,id),h=heif_image_get_component_height(im,id);size_t stride=0;const uint8_t* p=heif_image_get_component_readonly(im,id,&stride);
  printf("d%u,%u,%u,%u,%d,%d,%d,%zu;",id,w,h,heif_image_get_component_type(im,id),heif_image_get_component_channel(im,id),heif_image_get_component_datatype(im,id),heif_image_get_component_bits_per_pixel(im,id),stride);
  if(p)for(uint32_t y=0;y<h;y++)printf("%" PRIx64 ",",emit(p+y*stride,stride));
 }
}
int main(int argc,char** argv){
 if(argc!=2)return 2;pixels=fopen(argv[1],"wb");if(!pixels)return 2;
 uint32_t v[16];while(fread(v,sizeof(v),1,stdin)==1){
  uint32_t op=v[0],cs=v[1],ch=v[2],depth=v[3],w=v[4],h=v[5],layout=v[6],x=v[7],y=v[8],tw=v[9],th=v[10],repeat=v[11],lm=v[12];
  heif_image* im=NULL;uint32_t id;
  if(layout==0||layout==2||layout==7)im=create(w,h,cs,ch,depth,1,layout==2);
  else {
   error(heif_image_create(w,h,cs,ch,&im));
   if(layout==5||layout==6)error(heif_image_add_bayer_component(im,6,&id));
   if(layout!=1&&layout!=5){
    for(int c=0;c<(layout==3?2:1);c++){
     int kind=layout==8?65535:1,datatype=layout==4?2:layout==6?3:0;
     error(heif_image_add_component(im,w,h,kind,datatype,depth,&id));size_t stride;uint8_t* p=heif_image_get_component(im,id,&stride);
     if(p)for(uint32_t j=0;j<h;j++)for(size_t k=0;k<stride;k++)p[j*stride+k]=(uint8_t)(id*71+j*19+k*7);
    }
    error(heif_image_add_bayer_component(im,4,&id));
   }
  }
  if(!im)abort();if(layout==7)error(heif_image_add_component(im,w+1,h,8,0,depth,&id));
  metadata(im);printf("before:");snapshot(im,0);
  heif_context* context=heif_context_alloc();heif_security_limits limits=*heif_get_disabled_security_limits();
  limits.max_image_size_pixels=v[13];limits.max_memory_block_size=v[14];limits.max_total_memory=v[15];
  if(lm==2)limits.parent=heif_context_get_security_limits(context);
  if(lm==3)heif_context_set_security_limits(context,&limits);
  const heif_security_limits* lp=lm==0?NULL:lm==3?heif_context_get_security_limits(context):&limits;
  if(op==0){
   heif_image* out=(void*)(uintptr_t)0x1234;heif_error e=heif_image_extract_area(im,x,y,tw,th,lp,&out);error(e);printf("out%d,%d;",out==NULL,out==(void*)(uintptr_t)0x1234);
   if(!e.code){snapshot(out,0);error(heif_image_add_bayer_component(out,8,&id));printf("next%u;",id);heif_image_release(im);im=NULL;printf("retained:");snapshot(out,0);heif_image_release(out);}
  }else {
   error(op==1?heif_image_extend_padding_to_size(im,(int32_t)tw,(int32_t)th):heif_image_extend_to_size_fill_with_zero(im,tw,th));
   snapshot(im,th);
   if(repeat){error(op==1?heif_image_extend_padding_to_size(im,(int32_t)tw,(int32_t)th):heif_image_extend_to_size_fill_with_zero(im,tw,th));snapshot(im,th);}
  }
  if(im)heif_image_release(im);heif_context_free(context);puts("");
 }
 if(ferror(stdin)||ferror(stdout)||fclose(pixels))return 3;return 0;
}
