/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_sequences.h>
#include <libheif/heif_tai_timestamps.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){if(e.code)abort();printf(" e%d:%d:%s",e.code,e.subcode,e.message);}
static void string(const char* s){printf(" str%d:",s!=NULL);if(s)for(const unsigned char* p=(const unsigned char*)s;*p;p++)printf("%02x",*p);}
static void packet(const heif_tai_timestamp_packet* p){printf(" tai%d",p!=NULL);if(p)printf(":%u:%llu:%u:%u:%u",p->version,(unsigned long long)p->tai_timestamp,p->synchronization_state,p->timestamp_generation_failure,p->timestamp_is_modified);}
static void sample(heif_raw_sequence_sample* s){
 size_t n=999;const uint8_t* p=heif_raw_sequence_sample_get_data(s,&n);printf(" data%zu:%zu:%d:%d:",n,heif_raw_sequence_sample_get_data_size(s),p!=NULL,p==heif_raw_sequence_sample_get_data(s,NULL));for(size_t i=0;i<n;i++)printf("%02x",p[i]);
 printf(" duration%u present%d",heif_raw_sequence_sample_get_duration(s),heif_raw_sequence_sample_has_tai_timestamp(s));packet(heif_raw_sequence_sample_get_tai_timestamp(s));
 const char* id=heif_raw_sequence_sample_get_gimi_sample_content_id(s);const char* id2=heif_raw_sequence_sample_get_gimi_sample_content_id(s);string(id);printf(" independent%d",id!=id2);heif_string_release(id2);heif_string_release(id);
}
static void image(heif_image* p){printf(" image-duration%u",heif_image_get_duration(p));const char* id=heif_image_get_gimi_sample_content_id(p);string(id);heif_string_release(id);}
int main(void){
 uint32_t v[8];
 while(fread(v,sizeof(v),1,stdin)==1){
  heif_raw_sequence_sample* s=heif_raw_sequence_sample_alloc();if(!s)abort();sample(s);
  uint8_t data[4096];if(v[0]>sizeof(data))abort();for(size_t i=0;i<v[0];i++)data[i]=(uint8_t)(i*37+v[1]);
  error(heif_raw_sequence_sample_set_data(s,data,v[0]));memset(data,0,sizeof(data));heif_raw_sequence_sample_set_duration(s,v[1]);
  char id[256];for(unsigned i=0;i<255;i++)id[i]=(char)(1+(i+v[1])%255);id[v[2]%256]=0;
  heif_raw_sequence_sample_set_gimi_sample_content_id(s,(v[3]&1)?NULL:id);
  heif_tai_timestamp_packet* t=heif_tai_timestamp_packet_alloc();t->version=(uint8_t)v[4];t->tai_timestamp=((uint64_t)v[5]<<32)|v[1];t->synchronization_state=(uint8_t)v[6];t->timestamp_generation_failure=(uint8_t)(v[6]>>8);t->timestamp_is_modified=(uint8_t)(v[6]>>16);
  if(v[3]&2)t->version=0;heif_raw_sequence_sample_set_tai_timestamp(s,t);heif_tai_timestamp_packet_release(t);sample(s);
  const char* retained=heif_raw_sequence_sample_get_gimi_sample_content_id(s);
  heif_raw_sequence_sample_set_gimi_sample_content_id(s,"changed");error(heif_raw_sequence_sample_set_data(s,data,v[0]/2));sample(s);string(retained);
  t=heif_tai_timestamp_packet_alloc();heif_raw_sequence_sample_set_tai_timestamp(s,t);heif_tai_timestamp_packet_release(t);heif_raw_sequence_sample_set_gimi_sample_content_id(s,NULL);error(heif_raw_sequence_sample_set_data(s,data,0));sample(s);
  heif_raw_sequence_sample_release(s);string(retained);heif_string_release(retained);
  heif_image* p=NULL;error(heif_image_create(4,4,heif_colorspace_monochrome,heif_chroma_monochrome,&p));error(heif_image_add_plane(p,heif_channel_Y,4,4,8));image(p);heif_image_set_duration(p,v[1]);heif_image_set_gimi_sample_content_id(p,(v[3]&1)?NULL:id);memset(id,0,sizeof(id));image(p);
  heif_image* scaled=NULL;error(heif_image_scale_image(p,&scaled,2,2,NULL));if(scaled){image(scaled);heif_image_release(scaled);}error(heif_image_crop(p,1,0,0,1));image(p);heif_image_set_gimi_sample_content_id(p,NULL);image(p);heif_image_release(p);
  puts("");
 }
 /* A version-zero packet may be only its single-byte prefix. */
 heif_raw_sequence_sample* s=heif_raw_sequence_sample_alloc();uint8_t* prefix=malloc(1);*prefix=0;heif_raw_sequence_sample_set_tai_timestamp(s,(const heif_tai_timestamp_packet*)prefix);free(prefix);sample(s);heif_raw_sequence_sample_release(s);heif_raw_sequence_sample_release(NULL);puts("");
 return 0;
}
