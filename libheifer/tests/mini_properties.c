/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_properties.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e) {
  printf(" error=%d:%d:", e.code, e.subcode);
  if(e.message) for(const unsigned char* p=(const unsigned char*)e.message;*p;p++) printf("%02x",*p);
}
static void snapshot(heif_image_handle* h) {
  heif_content_light_level cl;heif_mastering_display_colour_volume md;heif_ambient_viewing_environment am;
  memset(&cl,0xa5,sizeof(cl));memset(&md,0xa5,sizeof(md));memset(&am,0xa5,sizeof(am));
  printf(" has=%d,%d,%d,%d",heif_image_handle_has_content_light_level(h),heif_image_handle_has_mastering_display_colour_volume(h),heif_image_handle_has_ambient_viewing_environment(h),heif_image_handle_has_nominal_diffuse_white_luminance(h));
  printf(" cl=%d",heif_image_handle_get_content_light_level(h,&cl));printf(":%u,%u",cl.max_content_light_level,cl.max_pic_average_light_level);
  printf(" md=%d",heif_image_handle_get_mastering_display_colour_volume(h,&md));
  for(int i=0;i<3;i++)printf(":%u,%u",md.display_primaries_x[i],md.display_primaries_y[i]);
  printf(":%u,%u,%u,%u",md.white_point_x,md.white_point_y,md.max_display_mastering_luminance,md.min_display_mastering_luminance);
  printf(" am=%d",heif_image_handle_get_ambient_viewing_environment(h,&am));printf(":%u,%u,%u",am.ambient_illumination,am.ambient_light_x,am.ambient_light_y);
  printf(" nd=%u",heif_image_handle_get_nominal_diffuse_white_luminance(h));
}
int main(void) {
  uint32_t length;
  while(fread(&length,4,1,stdin)==1) {
    if(length>2000000)return 1;
    void* data=malloc(length+1);if(fread(data,1,length,stdin)!=length)return 2;
    for(int copy=0;copy<2;copy++) {
      heif_context* ctx=heif_context_alloc();
      heif_error e=copy?heif_context_read_from_memory(ctx,data,length,NULL):heif_context_read_from_memory_without_copy(ctx,data,length,NULL);
      error(e);
      if(!e.code) {
        for(uint32_t id=1;id<=2;id++) {
          uint32_t props[64];for(int i=0;i<64;i++)props[i]=0xa5a5a5a5;
          int count=heif_item_get_properties_of_type(ctx,id,0,props,64);
          printf(" item=%u count=%d",id,count);
          for(int i=0;i<count;i++)printf(" %u:%x",props[i],heif_item_get_property_type(ctx,id,props[i]));
          heif_image_handle* h=NULL;error(heif_context_get_image_handle(ctx,id,&h));
          if(h){snapshot(h);heif_image_handle_release(h);}
        }
      }
      heif_context_free(ctx);puts("");
    }
    free(data);
  }
  return ferror(stdin)||ferror(stdout)?3:0;
}
