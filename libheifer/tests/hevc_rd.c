/* SPDX-License-Identifier: LGPL-3.0-or-later */
// enc: rd enc W H QUALITY in.rgb out.heic [encoder-id]   (RD_FORMAT: compression format, default HEVC)
// dec: rd dec in.heic out.rgb   (prints W H)
#include <libheif/heif.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void chk(struct heif_error e){ if(e.code){fprintf(stderr,"err %d %d %s\n",e.code,e.subcode,e.message);exit(1);} }
int main(int c,char**v){
  if(!strcmp(v[1],"enc")){
    int w=atoi(v[2]),h=atoi(v[3]),q=atoi(v[4]);
    FILE*f=fopen(v[5],"rb");unsigned char*rgb=malloc(w*h*3);if(fread(rgb,1,w*h*3,f)!=(size_t)(w*h*3))return 1;fclose(f);
    struct heif_image*img;chk(heif_image_create(w,h,heif_colorspace_RGB,heif_chroma_interleaved_RGB,&img));
    chk(heif_image_add_plane(img,heif_channel_interleaved,w,h,8));
    int stride;uint8_t*p=heif_image_get_plane(img,heif_channel_interleaved,&stride);
    for(int y=0;y<h;y++)memcpy(p+y*stride,rgb+y*w*3,w*3);
    struct heif_context*ctx=heif_context_alloc();
    const struct heif_encoder_descriptor*d[8];const char*format=getenv("RD_FORMAT");
    int n=heif_get_encoder_descriptors(format?(enum heif_compression_format)atoi(format):heif_compression_HEVC,c>7?v[7]:NULL,d,8);
    if(n<1){fprintf(stderr,"no encoder\n");return 1;}
    fprintf(stderr,"encoder: %s\n",heif_encoder_descriptor_get_name(d[0]));
    struct heif_encoder*enc;chk(heif_context_get_encoder(ctx,d[0],&enc));
    chk(heif_encoder_set_lossy_quality(enc,q));
    struct heif_image_handle*hd;chk(heif_context_encode_image(ctx,img,enc,NULL,&hd));
    chk(heif_context_write_to_file(ctx,v[6]));
    return 0;
  }
  struct heif_context*ctx=heif_context_alloc();chk(heif_context_read_from_file(ctx,v[2],NULL));
  struct heif_image_handle*hd;chk(heif_context_get_primary_image_handle(ctx,&hd));
  struct heif_image*img;chk(heif_decode_image(hd,&img,heif_colorspace_RGB,heif_chroma_interleaved_RGB,NULL));
  int w=heif_image_get_width(img,heif_channel_interleaved),h=heif_image_get_height(img,heif_channel_interleaved),stride;
  const uint8_t*p=heif_image_get_plane_readonly(img,heif_channel_interleaved,&stride);
  FILE*f=fopen(v[3],"wb");for(int y=0;y<h;y++)fwrite(p+y*stride,1,w*3,f);fclose(f);printf("%d %d\n",w,h);return 0;
}
