/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main context_original_main
#include "context.c"
#undef main
#include <unistd.h>
#include <libheif/heif_items.h>
static void mask_pixels(heif_image_handle* h){heif_image* img=(void*)(uintptr_t)0x1234;heif_error e=heif_decode_image(h,&img,heif_colorspace_undefined,heif_chroma_undefined,NULL);err(e);printf(" decoded%d,%d",img==NULL,img==(void*)(uintptr_t)0x1234);if(!e.code){int stride;const uint8_t* p=heif_image_get_plane_readonly(img,heif_channel_Y,&stride);int w=heif_image_get_width(img,heif_channel_Y),h=heif_image_get_height(img,heif_channel_Y),b=heif_image_get_bits_per_pixel(img,heif_channel_Y);printf(" plane%d,%d,%d:",w,h,b);for(int y=0;y<h;y++)for(int x=0;x<w*((b+7)/8);x++)printf("%02x",p[y*stride+x]);heif_image_release(img);}}
int main(int argc,char**argv){if(argc<2)return 2;uint32_t length;while(fread(&length,4,1,stdin)==1){if(length>2000000)return 2;unsigned char* data=malloc((size_t)length+1);if(fread(data,1,length,stdin)!=length)return 2;
 FILE* f=fopen(argv[1],"wb");if(!f)return 3;if(fwrite(data,1,length,f)!=length)return 3;fclose(f);
 heif_context* ctx=heif_context_alloc();err(heif_context_read_from_file(ctx,argv[1],NULL));printf(" count%d",heif_context_get_number_of_top_level_images(ctx));uint32_t id=0x12345678;err(heif_context_get_primary_image_ID(ctx,&id));printf(" id%u",id);heif_image_handle* h=(void*)(uintptr_t)0x1234;heif_error e=heif_context_get_primary_image_handle(ctx,&h);err(e);printf(" out%d,%d",h==NULL,h==(void*)(uintptr_t)0x1234);
 int mask=0;for(size_t i=0;i+4<=length;i++)if(!memcmp(data+i,"mskC",4))mask=1;
 if(unlink(argv[1]))return 3;if(!e.code){image(h);if(mask){uint8_t* payload=NULL;size_t size=0;enum heif_metadata_compression compression;err(heif_item_get_item_data(ctx,id,&compression,&payload,&size));printf(" payload%d,%zu:",compression,size);for(size_t i=0;i<size;i++)printf("%02x",payload[i]);heif_release_item_data(ctx,&payload);mask_pixels(h);}}
 err(heif_context_read_from_file(ctx,argv[1],NULL));printf(" missing-count%d missing-items%d",heif_context_get_number_of_top_level_images(ctx),(int)heif_context_get_number_of_items(ctx));err(heif_context_get_primary_image_ID(ctx,&id));printf(" missing-id%u",id);heif_context_free(ctx);if(!e.code){image(h);heif_image_handle_release(h);}free(data);puts("");}
 return 0;}
