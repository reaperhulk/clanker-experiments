/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static void error(heif_error e) {
  printf(" %d:%d:", e.code, e.subcode);
  for (const unsigned char* p=(const unsigned char*)e.message; *p; p++) printf("%02x", *p);
}
static heif_error trigger(heif_context* c, heif_image_handle* h, unsigned op) {
  unsigned char data[256];
  heif_image* image=NULL;
  heif_image_handle* thumbnail=NULL;
  switch(op) {
    case 0: return heif_image_handle_get_preferred_decoding_colorspace(h,NULL,NULL);
    case 1: return heif_image_handle_get_thumbnail(h,999,&thumbnail);
    case 2: return heif_image_handle_get_metadata(h,999,data);
    case 3: return heif_image_handle_get_raw_color_profile(h,data);
    case 4: {
      heif_error e=heif_decode_image(h,&image,heif_colorspace_RGB,heif_chroma_444,NULL);
      heif_image_release(image); return e;
    }
    default: return heif_context_get_primary_image_ID(c,NULL);
  }
}
int main(void) {
  uint32_t size;
  if(fread(&size,4,1,stdin)!=1 || size>2000000) return 1;
  void* data=malloc(size);
  if(fread(data,1,size,stdin)!=size) return 2;
  for(unsigned a=0;a<6;a++) for(unsigned b=0;b<6;b++) for(unsigned release=0;release<4;release++) {
    heif_context* c=heif_context_alloc();
    heif_error e=heif_context_read_from_memory(c,data,size,NULL);if(e.code)return 3;
    heif_image_handle *one=NULL,*alias=NULL,*two=NULL;
    if(heif_context_get_image_handle(c,1,&one).code || heif_context_get_image_handle(c,1,&alias).code || heif_context_get_image_handle(c,2,&two).code)return 4;
    heif_error first=trigger(c,one,a);
    printf("%u:%u:%u",a,b,release);error(first);
    /* The same context operation would invalidate its own prior error. */
    heif_error second;
    if(a==5 && b==5) second=trigger(c,two,0);else second=trigger(c,two,b);
    error(second);error(first);
    if(release>=1){heif_image_handle_release(one);one=NULL;}
    if(release>=2){heif_image_handle_release(two);two=NULL;}
    if(release>=3){heif_context_free(c);c=NULL;}
    /* alias keeps the first image and context alive through every release. */
    error(first);puts("");
    heif_image_handle_release(one);heif_image_handle_release(alias);heif_image_handle_release(two);heif_context_free(c);
  }
  free(data);return 0;
}
