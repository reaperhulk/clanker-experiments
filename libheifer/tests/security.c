/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <inttypes.h>
#include <limits.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define FIELDS(F) F(max_image_size_pixels) F(max_number_of_tiles) F(max_bayer_pattern_pixels) F(max_items) F(max_color_profile_size) F(max_memory_block_size) F(max_components) F(max_iloc_extents_per_item) F(max_size_entity_group) F(max_children_per_box) F(max_total_memory) F(max_sample_description_box_entries) F(max_sample_group_description_box_entries) F(max_sequence_frames) F(max_number_of_file_brands) F(max_bad_pixels) F(max_iso23001_17_pixel_size_bytes)
static void err(heif_error e) { printf(" %d:%d:", e.code,e.subcode); for(const unsigned char* s=(const unsigned char*)e.message; s && *s; ++s) printf("%02x",*s); }
static void dump(const heif_security_limits* p) { printf(" v=%u",p->version);
#define PRINT(n) printf(":%"PRIu64,(uint64_t)p->n);
 FIELDS(PRINT)
#undef PRINT
 printf(":%d",p->parent==NULL);
}
static void planes(void) {
 uint64_t values[]={0,1,152,153,154,4110,4111,4112,8221,8222,8223,12333,12334};
 int depths[]={8,10,16,32};
 for(unsigned field=0;field<3;field++)for(unsigned owner=0;owner<4;owner++)for(unsigned v=0;v<sizeof(values)/sizeof(values[0]);v++)for(unsigned d=0;d<4;d++) {
  heif_context* context=heif_context_alloc();heif_security_limits* root=heif_context_get_security_limits(context);*root=*heif_get_disabled_security_limits();
  if(field==0)root->max_image_size_pixels=values[v];else if(field==1)root->max_memory_block_size=values[v];else root->max_total_memory=values[v];
  heif_security_limits local=*root;if(owner==2)local.parent=root;
  const heif_security_limits* limits=owner==0?root:owner==3?NULL:&local;
  heif_image* images[3];for(unsigned i=0;i<3;i++){heif_error e=heif_image_create(17,9,heif_colorspace_monochrome,heif_chroma_monochrome,&images[i]);if(e.code)abort();}
  printf("planes=%u:%u:%"PRIu64":%d",field,owner,values[v],depths[d]);
  heif_error first=heif_image_add_plane_safe(images[0],heif_channel_Y,17,9,depths[d],limits);err(first);
  err(heif_image_add_plane_safe(images[1],heif_channel_Y,17,9,depths[d],limits));err(first);
  heif_image_release(images[0]);
  err(heif_image_add_plane_safe(images[2],heif_channel_Y,17,9,depths[d],limits));
  err(heif_image_add_plane_safe(images[2],heif_channel_Alpha,17,9,depths[d],limits));
  printf(" channels=%d:%d:%d",heif_image_has_channel(images[1],heif_channel_Y),heif_image_has_channel(images[2],heif_channel_Y),heif_image_has_channel(images[2],heif_channel_Alpha));
  heif_context_free(context);heif_image_release(images[1]);heif_image_release(images[2]);puts("");
 }
}
static void api(void) {
 printf("defaults");dump(heif_get_global_security_limits());dump(heif_get_disabled_security_limits());printf(" pointers=%d:%d:%d",heif_get_global_security_limits()==heif_get_global_security_limits(),heif_get_disabled_security_limits()==heif_get_disabled_security_limits(),heif_context_get_security_limits(NULL)==NULL);puts("");
 heif_context* c=heif_context_alloc();err(heif_context_set_security_limits(NULL,NULL));err(heif_context_set_security_limits(c,NULL));puts("");
 for(unsigned version=0;version<256;version++) {
  heif_security_limits input=*heif_get_disabled_security_limits();input.version=version;input.parent=(void*)(uintptr_t)0x1234;unsigned counter=1;
#define SET(n) input.n=(uint64_t)counter++*1000001;
  FIELDS(SET)
#undef SET
  size_t size=version<2?offsetof(heif_security_limits,max_total_memory):version<3?offsetof(heif_security_limits,max_sequence_frames):version<4?offsetof(heif_security_limits,max_bad_pixels):offsetof(heif_security_limits,parent);
  void* prefix=malloc(size);memcpy(prefix,&input,size);
  for(unsigned dst=0;dst<5;dst++) {
   heif_security_limits* p=heif_context_get_security_limits(c);*p=*heif_get_disabled_security_limits();p->version=dst;
   printf("copy=%u:%u",version,dst);err(heif_context_set_security_limits(c,prefix));dump(p);printf(" stable=%d",p==heif_context_get_security_limits(c));puts("");
  }
  free(prefix);
  heif_security_limits* p=heif_context_get_security_limits(c);*p=input;printf("alias=%u",version);err(heif_context_set_security_limits(c,p));dump(p);puts("");
 }
 int widths[]={INT_MIN,-65536,-32768,-1,0,1,32768,65536,INT_MAX};
 for(unsigned i=0;i<sizeof(widths)/sizeof(widths[0]);i++){heif_context_set_maximum_image_size_limit(c,widths[i]);printf("width=%d",widths[i]);dump(heif_context_get_security_limits(c));puts("");}
 heif_context_free(c);
}
int main(void) {
 api(); planes(); uint32_t n,field,phase;uint64_t value;
 while(fread(&n,4,1,stdin)==1) {
  if(n>2000000 || fread(&field,4,1,stdin)!=1 || fread(&phase,4,1,stdin)!=1 || fread(&value,8,1,stdin)!=1)return 1;
  unsigned char* bytes=malloc((size_t)n+1);if(fread(bytes,1,n,stdin)!=n)return 2;
  heif_context* c=heif_context_alloc();heif_context_set_max_decoding_threads(c,0);heif_security_limits* limits=heif_context_get_security_limits(c);
  for(unsigned when=0;when<2;when++) {
   if(when==phase) { unsigned index=0;
#define SET_FIELD(name) if(field==index++)limits->name=value;
    FIELDS(SET_FIELD)
#undef SET_FIELD
   }
   if(when==0){printf("input=%u:%u:%"PRIu64,field,phase,value);err(heif_context_read_from_memory(c,bytes,n,NULL));printf(" count=%d",heif_context_get_number_of_top_level_images(c));}
  }
  heif_image_handle* handle=NULL;heif_error e=heif_context_get_primary_image_handle(c,&handle);err(e);
  if(!e.code){heif_context* alias=heif_image_handle_get_context(handle);printf(" shared=%d",limits==heif_context_get_security_limits(alias));heif_context_free(c);c=alias;
   heif_decoding_options* options=heif_decoding_options_alloc();options->strict_decoding=1;options->output_image_nclx_profile_passthrough=1;
   heif_image* image=(void*)(uintptr_t)0x1234;e=heif_decode_image(handle,&image,heif_colorspace_undefined,heif_chroma_undefined,options);err(e);printf(" out=%d:%d",image==NULL,image==(void*)(uintptr_t)0x1234);
   if(!e.code){printf(" image=%d:%d",heif_image_get_primary_width(image),heif_image_get_primary_height(image));heif_image_release(image);}heif_decoding_options_free(options);heif_image_handle_release(handle);
  }
  heif_context_free(c);free(bytes);puts("");
 }
 return ferror(stdin)?3:0;
}
