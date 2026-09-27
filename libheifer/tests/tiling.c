/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_tiling.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void err(heif_error e) {printf(" e%d,%d,",e.code,e.subcode);if(e.message) {for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);} else printf("NULL");}
static void tiling(const heif_image_handle* h,int process) {
  heif_image_tiling t; memset(&t,0xA5,sizeof t);
  err(heif_image_handle_get_image_tiling(h,process,&t));
  printf(" t%d,%u,%u,%u,%u,%u,%u,%u,%u,%u",t.version,t.num_columns,t.num_rows,t.tile_width,t.tile_height,t.image_width,t.image_height,t.top_offset,t.left_offset,t.number_of_extra_dimensions);
  for(int i=0;i<8;i++) printf(",%u",t.extra_dimension_size[i]);
}
int main(void) {
  setvbuf(stdout,NULL,_IONBF,0); uint32_t args[5];
  while(fread(args,sizeof args,1,stdin)==1) {
    uint8_t* data=malloc(args[0]); if(fread(data,1,args[0],stdin)!=args[0]) return 2;
    heif_context* c=heif_context_alloc(); heif_error e=heif_context_read_from_memory(c,data,args[0],NULL); err(e);free(data);
    if(!e.code) {
      heif_image_handle* h=NULL;err(heif_context_get_primary_image_handle(c,&h));
      if(h) {
        if(args[4]) heif_context_get_security_limits(c)->max_image_size_pixels=args[4];
        tiling(h,args[1]); uint32_t id=0xBADF00D;err(heif_image_handle_get_grid_image_tile_id(h,args[1],args[2],args[3],&id));printf(" id%u",id);
        heif_decoding_options* opts=heif_decoding_options_alloc();opts->ignore_transformations=!args[1];
        heif_image* image=(void*)(uintptr_t)1;err(heif_image_handle_decode_image_tile(h,&image,heif_colorspace_undefined,heif_chroma_undefined,opts,args[2],args[3]));
        printf(" out%d",image==NULL?0:image==(void*)(uintptr_t)1?1:2);
        if(image&&image!=(void*)(uintptr_t)1) {
          for(int ch=0;ch<=10;ch++) {
            if(!heif_image_has_channel(image,(heif_channel)ch))continue;
            int w=heif_image_get_width(image,(heif_channel)ch),ht=heif_image_get_height(image,(heif_channel)ch),bits=heif_image_get_bits_per_pixel(image,(heif_channel)ch),stride=0;
            const uint8_t* p=heif_image_get_plane_readonly(image,(heif_channel)ch,&stride);
            printf(" pixels%d,%d,%d,%d:",ch,w,ht,bits);
            if(p)for(int y=0;y<ht;y++)for(int x=0;x<w*((bits+7)/8);x++)printf("%02x",p[y*stride+x]);
          }
          heif_image_release(image);
        }
        heif_decoding_options_free(opts);
        heif_image_handle_release(h);
      }
    }
    heif_context_free(c); puts("");
  }
  tiling(NULL,1);uint32_t id=123;err(heif_image_handle_get_grid_image_tile_id(NULL,1,0,0,&id));printf(" id%u\n",id);
  return 0;
}
