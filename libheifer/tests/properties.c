/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_properties.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void bytes(const void* data,size_t n){const unsigned char* p=data;for(size_t i=0;i<n;i++)printf("%02x",p[i]);}
static void error(heif_error e){printf(" e%d:%d:",e.code,e.subcode);bytes(e.message,strlen(e.message));}
static void description(const heif_property_user_description* d){printf(" v%d:",d->version);const char* strings[]={d->lang,d->name,d->description,d->tags};for(unsigned j=0;j<4;j++){bytes(strings[j],strlen(strings[j]));putchar(':');}}
static void snapshot(heif_context* c,uint32_t id){
  printf(" item%u",id);uint32_t kinds[]={0,heif_fourcc('u','d','e','s'),heif_fourcc('i','r','o','t'),heif_fourcc('u','u','i','d'),heif_fourcc('r','a','w','!')};int counts[]={-1,0,1,8};
  for(unsigned k=0;k<5;k++)for(unsigned n=0;n<4;n++){
    uint32_t ids[8]={999,999,999,999,999,999,999,999};printf(" list%d:%d",heif_item_get_properties_of_type(c,id,kinds[k],NULL,counts[n]),heif_item_get_properties_of_type(c,id,kinds[k],ids,counts[n]));for(unsigned j=0;j<8;j++)printf(":%u",ids[j]);
    for(unsigned j=0;j<8;j++)ids[j]=999;printf(" trans%d:%d",heif_item_get_transformation_properties(c,id,NULL,counts[n]),heif_item_get_transformation_properties(c,id,ids,counts[n]));for(unsigned j=0;j<8;j++)printf(":%u",ids[j]);
  }
  for(unsigned property=0;property<10;property++){
    printf(" p%u:%08x:%d:%d",property,heif_item_get_property_type(c,id,property),heif_item_get_property_transform_mirror(c,id,property),heif_item_get_property_transform_rotation_ccw(c,id,property));
    size_t size=999;heif_error e=heif_item_get_property_raw_size(c,id,property,&size);error(e);printf(" size%zu",size);
    unsigned char data[256];memset(data,0xa5,sizeof(data));if(!e.code&&size>sizeof(data))return;
    error(heif_item_get_property_raw_data(c,id,property,data));printf(" data");bytes(data,!e.code?size:4);
    memset(data,0xa5,16);error(heif_item_get_property_uuid_type(c,id,property,data));bytes(data,16);
    heif_property_user_description* d=(void*)(uintptr_t)0x1234;e=heif_item_get_property_user_description(c,id,property,&d);error(e);printf(" out%d:%d",d==NULL,d==(void*)(uintptr_t)0x1234);if(!e.code){description(d);heif_property_user_description_release(d);}
    int dims[][2]={{0,8},{8,0},{-1,8},{8,-1},{1,1},{8,8},{17,9},{2147483647,2147483647}};
    for(unsigned i=0;i<8;i++){int a=999,b=999,d=999,f=999;heif_item_get_property_transform_crop_borders(c,id,property,dims[i][0],dims[i][1],&a,&b,&d,&f);printf(" crop%d:%d:%d:%d",a,b,d,f);heif_item_get_property_transform_crop_borders(c,id,property,dims[i][0],dims[i][1],NULL,NULL,NULL,NULL);}
  }
  error(heif_item_get_property_user_description(c,id,0,NULL));error(heif_item_get_property_raw_size(c,id,0,NULL));error(heif_item_get_property_raw_data(c,id,0,NULL));error(heif_item_get_property_uuid_type(c,id,0,NULL));
}
static void additions(heif_context* c,unsigned variant){
  int versions[]={-2147483647-1,-1,0,1,2,2147483647};char name[]="label";
  heif_property_user_description d={versions[variant%6],(variant&1)?NULL:"en",(variant&2)?NULL:name,(variant&4)?NULL:"description",(variant&8)?NULL:"one,two"};
  uint32_t id=999;error(heif_item_add_property_user_description(c,1,&d,&id));printf(" add%u",id);name[0]='X';error(heif_item_add_property_user_description(c,1,&d,&id));printf(" add%u",id);error(heif_item_add_property_user_description(c,2,&d,&id));printf(" add%u",id);
  unsigned char uuid[16];for(unsigned i=0;i<16;i++)uuid[i]=(unsigned char)(i*17);unsigned char raw[]={0,0,0,0,'e','n',0,'N',0,'D',0,'T',0};
  uint32_t kinds[]={heif_fourcc('r','a','w','!'),heif_fourcc('u','u','i','d'),heif_fourcc('i','r','o','t'),heif_fourcc('u','d','e','s')};
  for(unsigned k=0;k<4;k++){id=999;error(heif_item_add_raw_property(c,1,kinds[k],uuid,raw,sizeof(raw),variant,&id));printf(" raw%u",id);error(heif_item_add_raw_property(c,1,kinds[k],uuid,raw,sizeof(raw),!variant,&id));printf(" duplicate%u",id);}
  d=(heif_property_user_description){-1,"en","N","D","T"};error(heif_item_add_property_user_description(c,1,&d,&id));printf(" dedup%u",id);
  error(heif_item_add_property_user_description(c,1,NULL,&id));error(heif_item_add_raw_property(c,1,kinds[0],NULL,NULL,0,0,&id));error(heif_item_add_raw_property(c,1,kinds[1],NULL,raw,0,0,&id));error(heif_item_add_raw_property(c,4294967295U,kinds[0],NULL,raw,0,0,NULL));
}
int main(void){
  for(unsigned v=0;v<16;v++){heif_context* c=heif_context_alloc();printf("memory%u",v);snapshot(c,1);additions(c,v);snapshot(c,1);snapshot(c,2);snapshot(c,4294967295U);
    heif_property_user_description* d=NULL;heif_error e=heif_item_get_property_user_description(c,1,0,&d);error(e);heif_context_free(c);if(!e.code){description(d);heif_property_user_description_release(d);}puts("");
  }
  uint32_t n;unsigned i=0;while(fread(&n,4,1,stdin)==1){if(n>2000000)return 1;void* data=malloc(n);if(fread(data,1,n,stdin)!=n)return 2;heif_context* c=heif_context_alloc();printf("file%u",i++);error(heif_context_read_from_memory(c,data,n,NULL));snapshot(c,1);snapshot(c,2);snapshot(c,99);additions(c,0);snapshot(c,1);error(heif_context_read_from_memory(c,NULL,0,NULL));snapshot(c,1);heif_context_free(c);free(data);puts("");}
  error(heif_item_get_property_user_description(NULL,0,0,NULL));error(heif_item_add_property_user_description(NULL,0,NULL,NULL));heif_property_user_description_release(NULL);puts("");return 0;
}
