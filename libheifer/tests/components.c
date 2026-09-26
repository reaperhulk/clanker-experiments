/* SPDX-License-Identifier: LGPL-3.0-or-later
 * Independent original-header client. Unknown-ID scalar getters that dereference
 * null upstream are excluded; storage getters and type queries define that case. */
#include <libheif/heif.h>
#include <libheif/heif_components.h>
#include <libheif/heif_properties.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

static void error(struct heif_error e) { printf("E%d/%d/%s;",e.code,e.subcode,e.message?e.message:"NULL"); }
static uint64_t hash(const uint8_t* p,size_t n) { uint64_t h=1469598103934665603ULL;for(size_t i=0;i<n;i++)h=(h^p[i])*1099511628211ULL;return h; }
static unsigned bytes_for(int bits) { if(bits<=8)return 1;if(bits<=16)return 2;if(bits<=32)return 4;if(bits<=64)return 8;return 16; }
#define TYPED(name,T) do { \
 size_t a=999,b=998;const T* rp=heif_image_get_component_##name##_readonly(image,id,&a); \
 T* wp=heif_image_get_component_##name(image,id,&b); \
 printf(#name ":%d,%d,%zu,%zu,%d,%d,%d;",rp!=NULL,wp!=NULL,a,b,(const void*)rp==(const void*)raw,(void*)wp==(void*)raw, \
 heif_image_get_component_##name##_readonly(image,id,NULL)==rp && heif_image_get_component_##name(image,id,NULL)==wp); \
 if(wp && write) { uint8_t pattern[sizeof(T)];for(size_t k=0;k<sizeof(T);k++)pattern[k]=(uint8_t)(id*23+k+sizeof(T)); \
 memcpy(wp,pattern,sizeof(T));printf("typed=%" PRIx64 ";",hash(raw,sizeof(T))); } \
}while(0)
static void storage(heif_image* image,uint32_t id,int write) {
 size_t a=999,b=998;const uint8_t* raw=heif_image_get_component_readonly(image,id,&a);uint8_t* writable=heif_image_get_component(image,id,&b);
 printf("raw=%d,%d,%zu,%zu,%d,%d,%d;",raw!=NULL,writable!=NULL,a,b,raw==writable,raw==heif_image_get_component_readonly(image,id,NULL),writable==heif_image_get_component(image,id,NULL));
 if(raw)printf("alignment=%zu;",(size_t)((uintptr_t)raw%16));
 TYPED(uint16,uint16_t);TYPED(uint32,uint32_t);TYPED(uint64,uint64_t);TYPED(int8,int8_t);TYPED(int16,int16_t);TYPED(int32,int32_t);TYPED(int64,int64_t);TYPED(float32,float);TYPED(float64,double);TYPED(complex32,heif_complex32);TYPED(complex64,heif_complex64);
}
static void snapshot(heif_image* image,int modify) {
 uint32_t n=heif_image_get_number_of_used_components(image),ids[40];for(unsigned i=0;i<40;i++)ids[i]=0xabcdef01;
 if(n>38)exit(3);heif_image_get_used_component_ids(image,ids);heif_image_get_used_component_ids(image,NULL);
 printf("N%u:",n);for(uint32_t i=0;i<=n;i++)printf("%u,",ids[i]);
 for(uint32_t j=0;j<n;j++) {
  uint32_t id=ids[j],w=heif_image_get_component_width(image,id),h=heif_image_get_component_height(image,id);int bits=heif_image_get_component_bits_per_pixel(image,id),channel=heif_image_get_component_channel(image,id);
  printf("D%u=%d,%u,%u,%d,%u,%d;",id,channel,w,h,bits,heif_image_get_component_type(image,id),heif_image_get_component_datatype(image,id));
  size_t stride=777;uint8_t* p=heif_image_get_component(image,id,&stride);
  if(p) {
    size_t legacy_stride=444;const uint8_t* legacy=heif_image_get_plane_readonly2(image,channel,&legacy_stride);
    printf("legacy=%d,%zu;",legacy==p,legacy_stride);
    printf("before=%" PRIx64 ";",hash(p,stride*h));
    if(modify)for(uint32_t y=0;y<h;y++)for(size_t x=0;x<stride;x++)p[y*stride+x]=(uint8_t)(id*31+y*13+x);
    printf("data=%" PRIx64 ";",hash(p,stride*h));
  }
  storage(image,id,modify);
  char text[]="gimi\xff identifier";error(heif_image_set_gimi_component_content_id(image,id,text));memset(text,0,sizeof(text));
  error(heif_image_set_gimi_component_content_id(image,id,""));error(heif_image_set_gimi_component_content_id(image,id,NULL));
 }
 for(unsigned i=0;i<2;i++) {uint32_t id=i?UINT32_MAX:0;printf("unknown=%u;",heif_image_get_component_type(image,id));storage(image,id,0);error(heif_image_set_gimi_component_content_id(image,id,"missing"));}
}
int main(void) {
 uint32_t v[9];unsigned index=0;
 while(fread(v,sizeof(v),1,stdin)==1) {
  uint32_t family=v[0],kind=v[1],datatype=v[2],depth=v[3],width=v[4],height=v[5],cs=v[6],chroma=v[7],mode=v[8];
  printf("case%u;",index++);heif_image* image=NULL;error(heif_image_create(7,5,cs,chroma,&image));if(!image){puts("");continue;}
  uint32_t id=0xeeeeeeee;
  if(family==2) {
    error(heif_image_add_bayer_component(image,(uint16_t)kind,&id));
    printf("ref=%u,%u,%d,%d,%u,%u,%d;",id,heif_image_get_number_of_used_components(image),heif_image_get_component_channel(image,id),heif_image_get_component_bits_per_pixel(image,id),heif_image_get_component_width(image,id),heif_image_get_component_height(image,id),heif_image_get_component_datatype(image,id));
    printf("type=%u;",heif_image_get_component_type(image,id));size_t stride=99;printf("data=%d;",heif_image_get_component(image,id,&stride)!=NULL);printf("stride=%zu;",stride);heif_image_release(image);puts("");continue;
  }
  error(heif_image_add_bayer_component(image,4,&id));printf("first=%u;",id);
  error(heif_image_add_component(image,(int32_t)width,(int32_t)height,(uint16_t)kind,(int32_t)datatype,(int32_t)depth,mode&1?NULL:&id));printf("add=%u;",id);
  if(family==1) {
    int channel=chroma>=10?heif_channel_interleaved:heif_channel_Y;
    error(heif_image_add_plane(image,channel,7,5,chroma>=12?12:8));
    error(heif_image_add_component(image,7,5,(uint16_t)kind,datatype,depth,&id));printf("repeat=%u;",id);
    error(heif_image_add_component(image,7,5,65535,heif_component_datatype_floating_point,32,NULL));
  }
  error(heif_image_add_bayer_component(image,6,&id));printf("last=%u;",id);
  snapshot(image,1);
  if(family==1 || (width==7 && height==5)) {
    heif_image* scaled=NULL;error(heif_image_scale_image(image,&scaled,4,3,NULL));if(scaled){printf("scaled:");snapshot(scaled,0);heif_image_release(scaled);}
    error(heif_image_crop(image,1,1,1,1));printf("crop:");snapshot(image,0);
    error(heif_image_add_bayer_component(image,8,&id));printf("aftercrop=%u;",id);
  }
  heif_image_release(image);puts("");
 }
 printf("null:");snapshot(NULL,0);printf("nullq=%d,%u,%u,%d,%u,%d;",heif_image_get_component_channel(NULL,1),heif_image_get_component_width(NULL,1),heif_image_get_component_height(NULL,1),heif_image_get_component_bits_per_pixel(NULL,1),heif_image_get_component_type(NULL,1),heif_image_get_component_datatype(NULL,1));
 error(heif_image_add_component(NULL,1,1,1,0,8,NULL));puts("");return 0;
}
