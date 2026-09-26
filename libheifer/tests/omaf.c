/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main transforms_main
#include "transforms.c"
#undef main
#include <libheif/heif_properties.h>
#include <libheif/heif_omaf.h>
static void snapshot(heif_image_handle* h,heif_image* image){
 if(h){printf("projection%d;",(int)heif_image_handle_get_omaf_image_projection(h));}
 else {
  printf("projection%d;",(int)heif_image_get_omaf_image_projection(image));
  heif_error warnings[16];int n=heif_image_get_decoding_warnings(image,0,warnings,16);printf("w%d;",n);for(int j=0;j<n&&j<16;j++)error(warnings[j]);dump(image);
  int values[]={0,1,31,32,255,-1,INT32_MIN,INT32_MAX};
  for(unsigned j=0;j<sizeof(values)/sizeof(values[0]);j++){
   heif_image_set_omaf_image_projection(image,(heif_omaf_image_projection)values[j]);printf("set%d;",(int)heif_image_get_omaf_image_projection(image));
   heif_image* scaled=NULL;heif_error e=heif_image_scale_image(image,&scaled,2,2,NULL);error(e);if(!e.code){printf("scaled%d;",(int)heif_image_get_omaf_image_projection(scaled));heif_image_release(scaled);}
  }
 }
}
static void props(heif_context* ctx){
 for(unsigned item=1;item<=2;item++){uint32_t ids[64];for(int j=0;j<64;j++)ids[j]=999;int n=heif_item_get_properties_of_type(ctx,item,0,ids,64);printf("properties%u,%d;",item,n);for(int j=0;j<n;j++)printf("%u=%x,",ids[j],heif_item_get_property_type(ctx,item,ids[j]));}
}
static void change(heif_image_handle* h,uint32_t seed,uint32_t flags){
 (void)flags;
 heif_image_handle_set_omaf_image_projection(h,(heif_omaf_image_projection)seed);
 heif_image_handle_set_omaf_image_projection(h,(heif_omaf_image_projection)seed);
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
