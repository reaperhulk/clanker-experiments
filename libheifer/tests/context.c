/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void text(const char* s) { if(!s) {printf("NULL");return;} for(;*s;s++) printf("%02x",(unsigned char)*s); }
static void err(heif_error e) { printf(" %d:%d:",e.code,e.subcode);text(e.message); }
static void image(const heif_image_handle* h) {
  printf(" image=%u:%d:%d:%d:%d:%d:%d:%d:%d:%d",heif_image_handle_get_item_id(h),heif_image_handle_is_primary_image(h),heif_image_handle_get_width(h),heif_image_handle_get_height(h),heif_image_handle_get_ispe_width(h),heif_image_handle_get_ispe_height(h),heif_image_handle_get_luma_bits_per_pixel(h),heif_image_handle_get_chroma_bits_per_pixel(h),heif_image_handle_has_alpha_channel(h),heif_image_handle_is_premultiplied_alpha(h));
  heif_colorspace cs=99;heif_chroma ch=99;err(heif_image_handle_get_preferred_decoding_colorspace(h,&cs,&ch));printf(" space=%d:%d",cs,ch);
  uint32_t a=123,b=456;int present=heif_image_handle_get_pixel_aspect_ratio(h,&a,&b);printf(" aspect=%d:%u:%u",present,a,b);
  heif_color_profile_nclx* n=(void*)(uintptr_t)0x1234;heif_error e=heif_image_handle_get_nclx_color_profile(h,&n);err(e);printf(" nclx=%u:%d:%d",(unsigned)heif_image_handle_get_color_profile_type(h),n==NULL,n==(void*)(uintptr_t)0x1234);
  if(!e.code) {printf(":%u:%d:%d:%d:%u",n->version,n->color_primaries,n->transfer_characteristics,n->matrix_coefficients,n->full_range_flag);heif_nclx_color_profile_free(n);}
  size_t raw=heif_image_handle_get_raw_color_profile_size(h);printf(" icc=%zu",raw);if(raw>1000000) exit(2);unsigned char* data=malloc(raw+2);memset(data,0xa5,raw+2);err(heif_image_handle_get_raw_color_profile(h,data+1));for(size_t i=0;i<raw+2;i++)printf("%02x",data[i]);free(data);
  err(heif_image_handle_get_raw_color_profile(h,NULL));err(heif_image_handle_get_nclx_color_profile(h,NULL));
  const char* filters[]={NULL,"Exif","mime","uri ","none"};
  for(unsigned f=0;f<sizeof(filters)/sizeof(filters[0]);f++) {
    uint32_t ids[34];for(unsigned i=0;i<34;i++)ids[i]=0xa5a5a5a5;
    int count=heif_image_handle_get_list_of_metadata_block_IDs(h,filters[f],ids+1,32);
    printf(" meta=%d:%d:%u:%u",heif_image_handle_get_number_of_metadata_blocks(h,filters[f]),count,ids[0],ids[33]);
    for(int i=1;i<=count;i++) {
      size_t size=heif_image_handle_get_metadata_size(h,ids[i]);if(size>1000000)exit(3);
      printf(" %u:%zu:",ids[i],size);text(heif_image_handle_get_metadata_type(h,ids[i]));printf(":");text(heif_image_handle_get_metadata_content_type(h,ids[i]));printf(":");text(heif_image_handle_get_metadata_item_uri_type(h,ids[i]));
      data=malloc(size+2);memset(data,0xa5,size+2);err(heif_image_handle_get_metadata(h,ids[i],data+1));for(size_t j=0;j<size+2;j++)printf("%02x",data[j]);free(data);err(heif_image_handle_get_metadata(h,ids[i],NULL));
    }
  }
  printf(" missing=%zu:",heif_image_handle_get_metadata_size(h,0xffffffff));text(heif_image_handle_get_metadata_type(h,0xffffffff));err(heif_image_handle_get_metadata(h,0xffffffff,NULL));
  int counts[]={-1,0,1,4};
  for(unsigned c=0;c<4;c++) {uint32_t ids[6]={11,12,13,14,15,16};int n=heif_image_handle_get_list_of_thumbnail_IDs(h,ids+1,counts[c]);printf(" thumbs=%d:%d",heif_image_handle_get_number_of_thumbnails(h),n);for(int i=0;i<6;i++)printf(":%u",ids[i]);}
  uint32_t ids[32];int nthumbs=heif_image_handle_get_list_of_thumbnail_IDs(h,ids,32);
  for(int i=0;i<nthumbs;i++){heif_image_handle* thumb=(void*)(uintptr_t)0x1234; e=heif_image_handle_get_thumbnail(h,ids[i],&thumb);err(e);printf(" thumb=%d:%d",thumb==NULL,thumb==(void*)(uintptr_t)0x1234);if(!e.code){printf(":%u:%d:%d",heif_image_handle_get_item_id(thumb),heif_image_handle_get_width(thumb),heif_image_handle_get_height(thumb));heif_image_handle_release(thumb);}}
  heif_image_handle* missing=(void*)(uintptr_t)0x1234;err(heif_image_handle_get_thumbnail(h,0xffffffff,&missing));printf(" absent-thumb=%d:%d",missing==NULL,missing==(void*)(uintptr_t)0x1234);err(heif_image_handle_get_thumbnail(h,0xffffffff,NULL));
}
int main(void) {
  uint32_t length;
  while(fread(&length,4,1,stdin)==1) {
    if(length>2000000)return 1;
    unsigned char* data=malloc((size_t)length+1);if(fread(data,1,length,stdin)!=length)return 2;
    for(int copy=0;copy<2;copy++) {
      heif_context* ctx=heif_context_alloc();uint32_t id=0x12345678;heif_image_handle* h=(void*)(uintptr_t)0x1234;
      printf("copy=%d empty=%d",copy,heif_context_get_number_of_top_level_images(ctx));err(heif_context_get_primary_image_ID(ctx,&id));err(heif_context_get_primary_image_handle(ctx,&h));printf(" empty-out=%u:%d",id,h==(void*)(uintptr_t)0x1234);
      unsigned char* owned=NULL; if(copy){owned=malloc((size_t)length+1);memcpy(owned,data,length);}
      heif_error e=copy?heif_context_read_from_memory(ctx,owned,length,NULL):heif_context_read_from_memory_without_copy(ctx,data,length,NULL);if(owned){memset(owned,0,length);free(owned);}err(e);
      printf(" count=%d",heif_context_get_number_of_top_level_images(ctx));
      err(heif_context_get_primary_image_ID(ctx,&id));printf(" primary=%u",id);err(heif_context_get_primary_image_ID(ctx,NULL));err(heif_context_get_primary_image_handle(ctx,NULL));
      int counts[]={-1,0,1,4};for(unsigned c=0;c<4;c++){uint32_t ids[6]={11,12,13,14,15,16};int count=heif_context_get_list_of_top_level_image_IDs(ctx,ids+1,counts[c]);printf(" list=%d",count);for(int i=0;i<6;i++)printf(":%u",ids[i]);}
      for(unsigned missing=0;missing<2;missing++){h=(void*)(uintptr_t)0x1234;e=heif_context_get_image_handle(ctx,missing?0xffffffff:0,&h);err(e);printf(" absent=%d:%d",h==NULL,h==(void*)(uintptr_t)0x1234);if(!e.code)heif_image_handle_release(h);}
      err(heif_context_get_image_handle(ctx,1,NULL));
      h=(void*)(uintptr_t)0x1234;e=heif_context_get_primary_image_handle(ctx,&h);err(e);printf(" handle=%d:%d",h==NULL,h==(void*)(uintptr_t)0x1234);
      if(!e.code) {
        heif_context* alias=heif_image_handle_get_context(h);heif_context_free(ctx);ctx=alias;
        printf(" alias=%d:%d",heif_context_get_number_of_top_level_images(ctx),heif_context_is_top_level_image_ID(ctx,id));
        image(h);
        /* A failed early read retains existing image handles/list. */
        err(heif_context_read_from_memory(ctx,"x",1,NULL));printf(" retained=%d:%d",heif_context_get_number_of_top_level_images(ctx),heif_image_handle_get_width(h));
        /* Successful reread replaces the context model; the old handle survives. */
        err(heif_context_read_from_memory(ctx,data,length,NULL));printf(" reread=%d:%d",heif_context_get_number_of_top_level_images(ctx),heif_image_handle_get_width(h));
        /* A non-picture metadata load does not replace an existing image model. */
        unsigned char* modified=malloc((size_t)length+1);memcpy(modified,data,length);
        for(size_t j=0;j+16<=length;j++) if(!memcmp(modified+j,"hdlr",4)){memcpy(modified+j+12,"null",4);break;}
        err(heif_context_read_from_memory(ctx,modified,length,NULL));printf(" nonpicture=%d",heif_context_get_number_of_top_level_images(ctx));
        memcpy(modified,data,length);
        /* Interpretation failures expose new partial state, retaining old handles. */
        for(size_t j=0;j+12<=length;j++) if(!memcmp(modified+j,"ispe",4)){memset(modified+j+8,0,4);break;}
        err(heif_context_read_from_memory(ctx,modified,length,NULL));printf(" bad-size=%d",heif_context_get_number_of_top_level_images(ctx));
        heif_image_handle* newer=(void*)(uintptr_t)0x1234;e=heif_context_get_primary_image_handle(ctx,&newer);err(e);
        if(!e.code){printf(" new-width=%d",heif_image_handle_get_width(newer));heif_image_handle_release(newer);}
        printf(" old-width=%d",heif_image_handle_get_width(h));free(modified);
        heif_context_free(ctx);ctx=NULL;image(h);heif_image_handle_release(h);
      }
      heif_context_free(ctx);puts("");
    }
    free(data);
  }
  return ferror(stdin)?3:0;
}
