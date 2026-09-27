/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static void err(heif_error e){printf(" %d:%d:",e.code,e.subcode);for(const unsigned char* s=(const unsigned char*)e.message;s&&*s;s++)printf("%02x",*s);}
static heif_image* decode(heif_image_handle* h){heif_image* out=NULL;heif_decoding_options* o=heif_decoding_options_alloc();o->output_image_nclx_profile_passthrough=1;o->strict_decoding=1;heif_error e=heif_decode_image(h,&out,heif_colorspace_undefined,heif_chroma_undefined,o);err(e);heif_decoding_options_free(o);return e.code?NULL:out;}
int main(void){uint32_t n,m;while(fread(&n,4,1,stdin)==1){if(fread(&m,4,1,stdin)!=1||n>2000000||m>2000000)return 1;unsigned char* a=malloc(n);unsigned char* b=malloc(m);if(fread(a,1,n,stdin)!=n||fread(b,1,m,stdin)!=m)return 2;
 uint64_t values[]={0,1,128,129,164,165,329,330,4110,4111,4112,4275,4276,4277,4440,4441,4442,8222,8387,8552,12333,12498,12663};
 for(unsigned hold=0;hold<3;hold++)for(unsigned v=0;v<sizeof(values)/sizeof(values[0]);v++){
  printf("reload=%u:%llu",hold,(unsigned long long)values[v]);heif_context* c=heif_context_alloc();heif_context_set_max_decoding_threads(c,0);err(heif_context_read_from_memory(c,a,n,NULL));
  heif_image_handle* first=NULL;heif_image_handle* second=NULL;heif_error e=heif_context_get_image_handle(c,1,&first);err(e);if(e.code)return 3;e=heif_context_get_image_handle(c,2,&second);err(e);if(e.code)return 4;
  heif_image* image=decode(first);heif_image_release(image);image=decode(second);heif_image_release(image);
  heif_context_get_security_limits(c)->max_memory_block_size=values[v];image=decode(first);heif_image_release(image);heif_context_get_security_limits(c)->max_memory_block_size=heif_get_global_security_limits()->max_memory_block_size;
  if(hold<2){heif_image_handle_release(second);second=NULL;}if(hold==0){heif_image_handle_release(first);first=NULL;}
  heif_context_get_security_limits(c)->max_total_memory=values[v];err(heif_context_read_from_memory(c,b,m,NULL));heif_image_handle* fresh=NULL;e=heif_context_get_primary_image_handle(c,&fresh);err(e);
  if(!e.code){image=decode(fresh);heif_image_release(image);image=decode(fresh);heif_image_release(image);heif_image_handle_release(fresh);}
  heif_image_handle_release(first);heif_image_handle_release(second);heif_context_free(c);puts("");
 }
 free(a);free(b);
}return 0;}
