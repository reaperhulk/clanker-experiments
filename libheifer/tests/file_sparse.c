/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
static void error(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
int main(int argc,char**argv){if(argc<3)return 2;if(atoi(argv[2])){struct rlimit lim={256u*1024u*1024u,256u*1024u*1024u};if(setrlimit(RLIMIT_AS,&lim))return 2;}heif_context*c=heif_context_alloc();error(heif_context_read_from_file(c,argv[1],NULL));printf(" count%d",heif_context_get_number_of_top_level_images(c));heif_image_handle*h=NULL;heif_error e=heif_context_get_primary_image_handle(c,&h);error(e);heif_context_free(c);if(e.code)return 3;printf(" size%d,%d",heif_image_handle_get_width(h),heif_image_handle_get_height(h));heif_image*img=NULL;e=heif_decode_image(h,&img,heif_colorspace_undefined,heif_chroma_undefined,NULL);error(e);heif_image_handle_release(h);if(e.code)return 3;int stride;const uint8_t*p=heif_image_get_plane_readonly(img,heif_channel_Y,&stride);printf(" pixels");for(int y=0;y<7;y++)for(int x=0;x<5;x++)printf("%02x",p[y*stride+x]);heif_image_release(img);puts("");return 0;}
