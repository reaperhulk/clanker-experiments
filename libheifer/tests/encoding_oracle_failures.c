/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Isolated upstream failures: these are NOT parity cases. */
#include <libheif/heif.h>
#include <libheif/heif_tai_timestamps.h>
#include <stdlib.h>
#include <stdio.h>
int main(int argc,char** argv) {
  if(argc!=2) return 2;
  int timestamp=atoi(argv[1]);
  heif_context* ctx=heif_context_alloc();
  heif_encoder* encoder=NULL;
  heif_error e=heif_context_get_encoder_for_format(ctx,timestamp?heif_compression_mask:heif_compression_uncompressed,&encoder);
  if(e.code) return 3;
  heif_image* image=NULL;
  e=heif_image_create(3,2,heif_colorspace_monochrome,heif_chroma_monochrome,&image);
  if(e.code) return 4;
  e=heif_image_add_plane(image,heif_channel_Y,3,2,8);
  if(e.code) return 5;
  if(timestamp) {
    heif_tai_timestamp_packet* t=heif_tai_timestamp_packet_alloc();
    t->tai_timestamp=123456789;
    e=heif_image_set_tai_timestamp(image,t);
    heif_tai_timestamp_packet_release(t);
    if(e.code) return 6;
  }
  heif_image_handle* handle=NULL;
  e=heif_context_encode_image(ctx,image,encoder,NULL,&handle);
  if(e.code) return 7;
  if(!timestamp) printf("alpha %d\n",heif_image_handle_has_alpha_channel(handle));
  heif_image_handle_release(handle);
  heif_image_release(image);
  heif_encoder_release(encoder);
  heif_context_free(ctx);
  return 0;
}
