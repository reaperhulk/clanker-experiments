/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
static void check(heif_error e) { if(e.code) { fprintf(stderr, "%s\n",e.message); exit(2); } }
static uint64_t now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t); return (uint64_t)t.tv_sec*1000000000ULL+t.tv_nsec; }
int main(int argc,char**argv) {
  if(argc!=3) return 1;
  unsigned iterations=(unsigned)strtoul(argv[2],NULL,10);
  if(!iterations) return 1;
  FILE*f=fopen(argv[1],"rb"); if(!f) return 1;
  if(fseek(f,0,SEEK_END)) return 1;
  long size=ftell(f); if(size<0) return 1;
  rewind(f); uint8_t*data=malloc((size_t)size); if(!data) return 1;
  if(fread(data,1,(size_t)size,f)!=(size_t)size) return 1;
  fclose(f);
  uint64_t elapsed=0,checksum=0;
  for(unsigned iteration=0;iteration<=iterations;++iteration) {
    uint64_t start=now();
    heif_context*ctx=heif_context_alloc();
    check(heif_context_read_from_memory_without_copy(ctx,data,(size_t)size,NULL));
    int count=heif_context_get_number_of_top_level_images(ctx);
    heif_item_id*ids=calloc((size_t)count,sizeof(*ids));
    heif_context_get_list_of_top_level_image_IDs(ctx,ids,count);
    for(int i=0;i<count;++i) {
      heif_image_handle*h=NULL;check(heif_context_get_image_handle(ctx,ids[i],&h));
      heif_decoding_options*o=heif_decoding_options_alloc();
      o->ignore_transformations=1;o->output_image_nclx_profile_passthrough=1;
      heif_image*image=NULL;
      check(heif_decode_image(h,&image,heif_colorspace_undefined,heif_chroma_undefined,o));
      size_t stride;const uint8_t*p=heif_image_get_plane_readonly2(image,heif_channel_Y,&stride);
      checksum+=p[0];
      heif_image_release(image);heif_image_handle_release(h);heif_decoding_options_free(o);
    }
    free(ids);heif_context_free(ctx);
    if(iteration) elapsed+=now()-start;
  }
  free(data);
  printf("{\"ns\":%llu,\"iterations\":%u,\"checksum\":%llu}\n",(unsigned long long)elapsed,iterations,(unsigned long long)checksum);
}
