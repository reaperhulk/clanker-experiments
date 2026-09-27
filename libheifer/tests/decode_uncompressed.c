/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_components.h>
static void inspect_release(heif_image* image);
#define heif_image_release inspect_release
#define main single_file_main
#include "decode.c"
#undef main
#undef heif_image_release
static unsigned successful;
static void inspect_release(heif_image* image){
  successful++;
  uint32_t n=heif_image_get_number_of_used_components(image);if(n>256)abort();number(n);
  uint32_t ids[256];heif_image_get_used_component_ids(image,ids);
  for(uint32_t i=0;i<n;i++){
    uint32_t id=ids[i];number(id);number(heif_image_get_component_type(image,id));
    uint32_t w=heif_image_get_component_width(image,id),h=heif_image_get_component_height(image,id);
    int b=heif_image_get_component_bits_per_pixel(image,id);number(w);number(h);number(b);number(heif_image_get_component_channel(image,id));number(heif_image_get_component_datatype(image,id));
    size_t stride=0;const uint8_t* p=heif_image_get_component_readonly(image,id,&stride);number(stride);number(p!=NULL);
    if(p){unsigned bytes=b<=8?1:b<=16?2:b<=32?4:b<=64?8:16;for(uint32_t y=0;y<h;y++)fwrite(p+y*stride,1,(size_t)w*bytes,output);}
    for(uint32_t j=0;j<i;j++)number(p==heif_image_get_component_readonly(image,ids[j],NULL));
  }
  heif_image_release(image);
}
static void handle(const heif_image_handle* h){
  number(heif_image_handle_get_width(h));number(heif_image_handle_get_height(h));number(heif_image_handle_get_luma_bits_per_pixel(h));number(heif_image_handle_get_chroma_bits_per_pixel(h));number(heif_image_handle_has_alpha_channel(h));
  heif_colorspace cs=777;heif_chroma ch=888;error(heif_image_handle_get_preferred_decoding_colorspace(h,&cs,&ch));number(cs);number(ch);
  uint32_t n=heif_image_handle_get_number_of_components(h),ids[256];if(n>256)abort();number(n);heif_image_handle_get_used_component_ids(h,ids);
  for(uint32_t i=0;i<n;i++){number(ids[i]);number(heif_image_handle_get_component_type(h,ids[i]));number(heif_image_handle_get_component_datatype(h,ids[i]));number(heif_image_handle_get_component_bits_per_pixel(h,ids[i]));}
}
int main(void){
  uint32_t header[2];
  while(fread(header,sizeof(header),1,stdin)==1){
    if(header[0]>4000000)abort();uint8_t* data=malloc(header[0]+1);if(fread(data,1,header[0],stdin)!=header[0])abort();
    output=tmpfile();if(!output)abort();successful=0;
    heif_context* ctx=heif_context_alloc();error(heif_context_read_from_memory(ctx,data,header[0],NULL));memset(data,0,header[0]);free(data);
    heif_image_handle* h=NULL;heif_error e=heif_context_get_primary_image_handle(ctx,&h);error(e);
    if(!e.code){handle(h);decode(h,header[1]);handle(h);}
    heif_context_free(ctx);if(h){handle(h);decode(h,header[1]);heif_image_handle_release(h);}
    long n=ftell(output);if(n<0 || n>100000000)abort();rewind(output);uint32_t frame[2]={(uint32_t)n,successful};fwrite(frame,sizeof(frame),1,stdout);char buf[8192];size_t count;while((count=fread(buf,1,sizeof(buf),output)))fwrite(buf,1,count,stdout);fclose(output);
  }
  return 0;
}
