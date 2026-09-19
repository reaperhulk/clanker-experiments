/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main transforms_main
#include "transforms.c"
#undef main
#include <libheif/heif_properties.h>
static void cl_print(heif_content_light_level v){printf("c%u,%u;",v.max_content_light_level,v.max_pic_average_light_level);}
static void md_print(heif_mastering_display_colour_volume v){printf("m");for(int j=0;j<3;j++)printf("%u,%u,",v.display_primaries_x[j],v.display_primaries_y[j]);printf("%u,%u,%u,%u;",v.white_point_x,v.white_point_y,v.max_display_mastering_luminance,v.min_display_mastering_luminance);}
static void am_print(heif_ambient_viewing_environment v){printf("a%u,%u,%u;",v.ambient_illumination,v.ambient_light_x,v.ambient_light_y);}
static void snapshot(heif_image_handle* h,heif_image* image){
 heif_content_light_level cl;heif_mastering_display_colour_volume md;heif_ambient_viewing_environment am;
 memset(&cl,0xa5,sizeof(cl));memset(&md,0xa5,sizeof(md));memset(&am,0xa5,sizeof(am));
 uint32_t a=999,b=999;int x,y;
 if(h){
  printf("has%d,%d,%d,%d;",heif_image_handle_has_content_light_level(h),heif_image_handle_has_mastering_display_colour_volume(h),heif_image_handle_has_ambient_viewing_environment(h),heif_image_handle_has_nominal_diffuse_white_luminance(h));
  x=heif_image_handle_get_content_light_level(h,&cl);y=heif_image_handle_get_content_light_level(h,NULL);printf("cl%d,%d;",x,y);cl_print(cl);
  x=heif_image_handle_get_mastering_display_colour_volume(h,&md);y=heif_image_handle_get_mastering_display_colour_volume(h,NULL);printf("md%d,%d;",x,y);md_print(md);
  x=heif_image_handle_get_ambient_viewing_environment(h,&am);y=heif_image_handle_get_ambient_viewing_environment(h,NULL);printf("am%d,%d;",x,y);am_print(am);
  printf("nd%u;",heif_image_handle_get_nominal_diffuse_white_luminance(h));x=heif_image_handle_get_pixel_aspect_ratio(h,&a,&b);printf("pa%d,%u,%u;",x,a,b);
 }else {
  printf("has%d,%d,%d,%d;",heif_image_has_content_light_level(image),heif_image_has_mastering_display_colour_volume(image),heif_image_has_ambient_viewing_environment(image),heif_image_has_nominal_diffuse_white_luminance(image));
  heif_image_get_content_light_level(image,&cl);cl_print(cl);if(heif_image_has_mastering_display_colour_volume(image))heif_image_get_mastering_display_colour_volume(image,&md);md_print(md);
  x=heif_image_get_ambient_viewing_environment(image,&am);printf("am%d;",x);am_print(am);printf("nd%u;",heif_image_get_nominal_diffuse_white_luminance(image));heif_image_get_pixel_aspect_ratio(image,&a,&b);printf("pa%u,%u;",a,b);
  heif_error warnings[16];x=heif_image_get_decoding_warnings(image,0,warnings,16);printf("w%d;",x);for(int j=0;j<x&&j<16;j++)error(warnings[j]);dump(image);
 }
}
static void props(heif_context* ctx){
 for(unsigned item=1;item<=2;item++){uint32_t ids[64];for(int j=0;j<64;j++)ids[j]=999;int n=heif_item_get_properties_of_type(ctx,item,0,ids,64);printf("properties%u,%d;",item,n);for(int j=0;j<n;j++)printf("%u=%x,",ids[j],heif_item_get_property_type(ctx,item,ids[j]));}
}
static void change(heif_image_handle* h,uint32_t seed,uint32_t flags){
 heif_content_light_level cl={(uint16_t)seed,(uint16_t)(seed>>16)};
 heif_mastering_display_colour_volume md={{seed,seed+1,seed+2},{seed+3,seed+4,seed+5},seed+6,seed+7,seed,seed+1};
 heif_ambient_viewing_environment am={seed,(uint16_t)(seed>>16),(uint16_t)seed};
 for(int j=0;j<2;j++){
  heif_image_handle_set_content_light_level(h,&cl);heif_image_handle_set_mastering_display_colour_volume(h,&md);heif_image_handle_set_ambient_viewing_environment(h,&am);
  heif_image_handle_set_nominal_diffuse_white_luminance(h,seed);heif_image_handle_set_pixel_aspect_ratio(h,seed,(flags&1)?seed:seed+1);
 }
 memset(&cl,0,sizeof(cl));memset(&md,0,sizeof(md));memset(&am,0,sizeof(am));
 heif_image_handle_set_content_light_level(h,NULL);heif_image_handle_set_mastering_display_colour_volume(h,NULL);heif_image_handle_set_ambient_viewing_environment(h,NULL);
}
static void decoded(heif_image_handle* h){
 for(int rgb=0;rgb<2;rgb++){
  heif_image* image=(void*)(uintptr_t)0x1234;heif_error e=heif_decode_image(h,&image,rgb?heif_colorspace_RGB:heif_colorspace_undefined,rgb?heif_chroma_interleaved_RGB:heif_chroma_undefined,NULL);
  error(e);printf("out%d,%d;",image==NULL,image==(void*)(uintptr_t)0x1234);if(!e.code){printf("decoded:");snapshot(NULL,image);heif_image_release(image);}
 }
}
int main(void){
 uint32_t v[4];while(fread(v,sizeof(v),1,stdin)==1){
  uint8_t* data=malloc(v[0]+v[1]);if(fread(data,1,v[0]+v[1],stdin)!=v[0]+v[1])return 2;
  heif_context* ctx=heif_context_alloc();error(heif_context_read_from_memory(ctx,data,v[0],NULL));props(ctx);
  heif_image_handle* h=NULL;error(heif_context_get_primary_image_handle(ctx,&h));
  if(h){
   heif_image_handle* alias=NULL;error(heif_context_get_primary_image_handle(ctx,&alias));snapshot(h,NULL);decoded(h);
   change(h,v[3],v[2]);snapshot(alias,NULL);props(ctx);decoded(h);
   change(alias,v[3]^0xffffffffu,v[2]);snapshot(h,NULL);props(ctx);decoded(h);
   error(heif_context_read_from_memory(ctx,data+v[0],v[1],NULL));snapshot(h,NULL);props(ctx);
   heif_image_handle* newer=NULL;error(heif_context_get_primary_image_handle(ctx,&newer));if(newer)snapshot(newer,NULL);
   if(v[2]&2){change(h,0,v[2]);snapshot(h,NULL);props(ctx);if(newer)snapshot(newer,NULL);}
   heif_context_free(ctx);ctx=NULL;snapshot(h,NULL);decoded(h);
   if(newer){snapshot(newer,NULL);decoded(newer);heif_image_handle_release(newer);}
   heif_image_handle_release(alias);heif_image_handle_release(h);
  }
  if(ctx)heif_context_free(ctx);free(data);puts("");
 }
 return ferror(stdout)||ferror(stdin)?1:0;
}
