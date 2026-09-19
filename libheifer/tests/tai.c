/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_tai_timestamps.h>
#include <libheif/heif_items.h>
#include <libheif/heif_properties.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){printf(" e%d:%d:",e.code,e.subcode);for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}
static void clock_print(const heif_tai_clock_info* c){if(!c){printf(" Cnull");return;}printf(" C%u:%" PRIu64 ":%u:%d:%u",c->version,c->time_uncertainty,c->clock_resolution,c->clock_drift_rate,c->clock_type);}
static void timestamp_print(const heif_tai_timestamp_packet* t){if(!t){printf(" Tnull");return;}printf(" T%u:%" PRIu64 ":%u:%u:%u",t->version,t->tai_timestamp,t->synchronization_state,t->timestamp_generation_failure,t->timestamp_is_modified);}
static void image_query(heif_image* image){heif_tai_timestamp_packet* t=(void*)1;error(heif_image_get_tai_timestamp(image,&t));timestamp_print(t);heif_tai_timestamp_packet_release(t);error(heif_image_get_tai_timestamp(image,NULL));}
static void item_query(heif_context* ctx,uint32_t id){heif_tai_clock_info* c=(void*)1;heif_tai_timestamp_packet* t=(void*)1;error(heif_item_get_property_tai_clock_info(ctx,id,&c));clock_print(c);error(heif_item_get_property_tai_timestamp(ctx,id,&t));timestamp_print(t);heif_tai_clock_info_release(c);heif_tai_timestamp_packet_release(t);error(heif_item_get_property_tai_clock_info(ctx,id,NULL));error(heif_item_get_property_tai_timestamp(ctx,id,NULL));}
static void set_props(heif_context* ctx,uint32_t id,heif_tai_clock_info* c,heif_tai_timestamp_packet* t){
 for(int k=0;k<3;k++){uint32_t p=999;error(heif_item_set_property_tai_clock_info(ctx,id,c,k==2?NULL:&p));printf(" p%u",p);p=999;error(heif_item_set_property_tai_timestamp(ctx,id,t,k==2?NULL:&p));printf(" p%u",p);}
 item_query(ctx,id);c->time_uncertainty^=1;t->tai_timestamp^=1;
 uint32_t p=999;error(heif_item_set_property_tai_clock_info(ctx,id,c,&p));printf(" p%u",p);p=999;error(heif_item_set_property_tai_timestamp(ctx,id,t,&p));printf(" p%u",p);item_query(ctx,id);
}
int main(void){uint32_t v[12];unsigned n=0;while(fread(v,sizeof(v),1,stdin)==1){
 unsigned char* file=malloc(v[9]?v[9]:1);if(fread(file,1,v[9],stdin)!=v[9])return 2;printf("case%u",n++);
 heif_tai_clock_info c={(uint8_t)v[1],((uint64_t)v[4]<<32)|v[3],v[5],(int32_t)v[6],(uint8_t)v[7]};
 heif_tai_timestamp_packet t={(uint8_t)v[1],c.time_uncertainty,(uint8_t)v[8],(uint8_t)(v[8]>>8),(uint8_t)(v[8]>>16)};
 if(v[0]==0){
  heif_tai_clock_info d;heif_tai_timestamp_packet u;memset(&d,0xa5,sizeof(d));memset(&u,0xa5,sizeof(u));d.version=u.version=v[2];
  heif_tai_clock_info_copy(&d,&c);heif_tai_timestamp_packet_copy(&u,&t);clock_print(&d);timestamp_print(&u);
  heif_tai_clock_info_copy(&d,&d);heif_tai_timestamp_packet_copy(&u,&u);clock_print(&d);timestamp_print(&u);
 }else if(v[0]==1){
  heif_image* image=NULL;error(heif_image_create(8,8,heif_colorspace_monochrome,heif_chroma_monochrome,&image));if(!image)return 3;error(heif_image_add_plane(image,heif_channel_Y,8,8,8));image_query(image);
  error(heif_image_set_tai_timestamp(image,&t));image_query(image);t.tai_timestamp^=2;image_query(image);
  heif_tai_timestamp_packet* owned=NULL;error(heif_image_get_tai_timestamp(image,&owned));error(heif_image_set_tai_timestamp(image,&t));image_query(image);
  heif_image* scaled=NULL;error(heif_image_scale_image(image,&scaled,4,4,NULL));if(scaled){image_query(scaled);heif_image_release(scaled);}error(heif_image_crop(image,1,2,1,2));image_query(image);heif_image_release(image);timestamp_print(owned);heif_tai_timestamp_packet_release(owned);
  heif_context* ctx=heif_context_alloc();uint32_t id=999;error(heif_context_add_item(ctx,"zzzz",NULL,0,&id));printf(" id%u",id);item_query(ctx,id);
  unsigned char raw[13]={0};for(int j=0;j<8;j++)raw[4+j]=(unsigned char)(t.tai_timestamp>>((7-j)*8));raw[12]=(t.synchronization_state?128:0)|(t.timestamp_generation_failure?64:0)|(t.timestamp_is_modified?32:0);uint32_t raw_id=999;
  if(v[10]==1){error(heif_item_add_raw_property(ctx,id,0x69746169,NULL,raw,13,0,&raw_id));printf(" raw%u",raw_id);}
  set_props(ctx,id,&c,&t);
  if(v[10]==2){error(heif_item_add_raw_property(ctx,id,0x69746169,NULL,raw,13,0,&raw_id));printf(" raw%u",raw_id);item_query(ctx,id);}
  // Different raw flag bytes can serialize identically, but typed equality
  // still distinguishes them before another property is appended.
  c.clock_type^=4;t.synchronization_state^=2;set_props(ctx,id,&c,&t);
  heif_tai_clock_info* oc=NULL;heif_tai_timestamp_packet* ot=NULL;error(heif_item_get_property_tai_clock_info(ctx,id,&oc));error(heif_item_get_property_tai_timestamp(ctx,id,&ot));
  item_query(ctx,0);item_query(ctx,999);set_props(ctx,999,&c,&t);heif_context_free(ctx);clock_print(oc);timestamp_print(ot);heif_tai_clock_info_release(oc);heif_tai_timestamp_packet_release(ot);
 }else{
  heif_context* ctx=heif_context_alloc();error(heif_context_read_from_memory(ctx,file,v[9],NULL));
  for(uint32_t id=0;id<5;id++)item_query(ctx,id);
  uint32_t ids[16];int count=heif_context_get_list_of_top_level_image_IDs(ctx,ids,16);
  heif_tai_clock_info* oc=NULL;heif_tai_timestamp_packet* ot=NULL;heif_image_handle* handle=NULL;
  if(count){error(heif_context_get_image_handle(ctx,ids[0],&handle));error(heif_item_get_property_tai_clock_info(ctx,ids[0],&oc));error(heif_item_get_property_tai_timestamp(ctx,ids[0],&ot));set_props(ctx,ids[0],&c,&t);}
  if(handle){for(int rgb=0;rgb<2;rgb++){heif_image* image=NULL;error(heif_decode_image(handle,&image,rgb?heif_colorspace_RGB:heif_colorspace_undefined,rgb?heif_chroma_interleaved_RGB:heif_chroma_undefined,NULL));if(image){image_query(image);heif_image_release(image);}}}
  if(v[10]==0)error(heif_context_read_from_memory(ctx,"invalid",7,NULL));else if(v[10]==1)error(heif_context_read_from_memory(ctx,file,v[9],NULL));item_query(ctx,1);heif_context_free(ctx);clock_print(oc);timestamp_print(ot);heif_tai_clock_info_release(oc);heif_tai_timestamp_packet_release(ot);
  if(handle){
   for(int ignore=0;ignore<2;ignore++)for(int rgb=0;rgb<2;rgb++){heif_image* image=NULL;heif_decoding_options* o=heif_decoding_options_alloc();o->ignore_transformations=ignore;error(heif_decode_image(handle,&image,rgb?heif_colorspace_RGB:heif_colorspace_undefined,rgb?heif_chroma_interleaved_RGB:heif_chroma_undefined,o));heif_decoding_options_free(o);if(image){image_query(image);error(heif_image_crop(image,0,0,0,0));image_query(image);heif_image_release(image);}}
   heif_image_handle_release(handle);
  }
 }
 free(file);puts("");}
 heif_tai_clock_info* c=heif_tai_clock_info_alloc();heif_tai_timestamp_packet* t=heif_tai_timestamp_packet_alloc();clock_print(c);timestamp_print(t);
 // Exact version-zero prefix objects, with no fields past the version byte.
 unsigned char* short_c=calloc(1,1);unsigned char* short_t=calloc(1,1);heif_tai_clock_info_copy(c,(void*)short_c);heif_tai_timestamp_packet_copy(t,(void*)short_t);heif_tai_clock_info_copy((void*)short_c,c);heif_tai_timestamp_packet_copy((void*)short_t,t);clock_print(c);timestamp_print(t);printf(" short%u:%u",*short_c,*short_t);free(short_c);free(short_t);
 heif_context* ctx=heif_context_alloc();uint32_t p=999;error(heif_item_set_property_tai_clock_info(NULL,0,c,&p));error(heif_item_set_property_tai_timestamp(NULL,0,t,&p));error(heif_item_set_property_tai_clock_info(ctx,0,NULL,&p));error(heif_item_set_property_tai_timestamp(ctx,0,NULL,&p));error(heif_item_get_property_tai_clock_info(NULL,0,&c));error(heif_item_get_property_tai_timestamp(NULL,0,&t));clock_print(c);timestamp_print(t);printf(" p%u",p);heif_context_free(ctx);heif_tai_clock_info_release(c);heif_tai_timestamp_packet_release(t);heif_tai_clock_info_release(NULL);heif_tai_timestamp_packet_release(NULL);puts("");return 0;
}
