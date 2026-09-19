/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Same client, upstream headers, separate reference/candidate processes. */
#include <libheif/heif.h>
#include <libheif/heif_items.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static FILE* samples;
static void bytes(const uint8_t* p,size_t n) {
  uint64_t size=n;
  if(fwrite(&size,sizeof(size),1,samples)!=1 || fwrite(p,1,n,samples)!=n)exit(20);
}

static void string(const char* s) { if (!s) { printf("NULL"); return; } for (; *s; ++s) printf("%02x", (unsigned char)*s); }
static void error(struct heif_error e) { printf(" %d:%d:", e.code, e.subcode); string(e.message); }
static uint64_t hash(const uint8_t* p,size_t n) { uint64_t h=1469598103934665603ULL; for(size_t i=0;i<n;++i)h=(h^p[i])*1099511628211ULL; return h; }
static void payload(struct heif_context* c, heif_item_id id) {
  for(unsigned mask=0;mask<8;++mask) {
    enum heif_metadata_compression compression=(enum heif_metadata_compression)12345;
    uint8_t* p=(void*)(uintptr_t)0x1234; size_t size=999999;
    struct heif_error e=heif_item_get_item_data(c,id,mask&1?&compression:NULL,mask&2?&p:NULL,mask&4?&size:NULL);
    error(e);printf(" d%u=%d/%zu/%d/%d",mask,compression,size,p==NULL,p==(void*)(uintptr_t)0x1234);
    if(p && p!=(void*)(uintptr_t)0x1234) { bytes(p,size);printf("/%016llx",(unsigned long long)hash(p,size)); heif_release_item_data(c,&p);printf("/free%d",p==NULL);heif_release_item_data(NULL,&p); }
  }
}
static void references(struct heif_context* c,heif_item_id from) {
  for(int i=-1;i<8;++i) {
    uint32_t kind=0x12345678;heif_item_id* ids=(void*)(uintptr_t)0x1234;
    size_t n=heif_context_get_item_references(c,from,i,&kind,&ids);
    printf(" r%d=%zu/%u/%d",i,n,kind,ids==(void*)(uintptr_t)0x1234);
    if(ids!=(void*)(uintptr_t)0x1234) { for(size_t j=0;j<n;++j)printf("/%u",ids[j]);heif_release_item_references(NULL,&ids);printf("/free%d",ids==NULL);heif_release_item_references(c,&ids); }
    printf("/count%zu",heif_context_get_item_references(c,from,i,NULL,NULL));
  }
}
static void language(struct heif_context* c,heif_item_id id) {
  char* p=(void*)(uintptr_t)0x1234;struct heif_error e=heif_item_get_property_extended_language(c,id,&p);error(e);printf(" lang%d=",p==(void*)(uintptr_t)0x1234);
  if(!e.code) { string(p);heif_string_release(p); }
}
static void inspect(struct heif_context* c) {
  int n=heif_context_get_number_of_items(c);if(n<0||n>2000)exit(10);
  printf(" N%d",n);heif_item_id* ids=calloc((size_t)n+3,sizeof(*ids));if(!ids)exit(11);
  int caps[]={-1,0,1,n-1,n,n+1};
  for(unsigned j=0;j<sizeof(caps)/sizeof(*caps);++j) {
    for(int k=0;k<n+3;++k)ids[k]=0x12345678;
    int count=heif_context_get_list_of_item_IDs(c,ids+1,caps[j]);printf(" ids%d=%d/%u",caps[j],count,ids[0]);
    for(int k=1;k<n+3;++k)printf("/%u",ids[k]);
  }
  printf(" nullids%d",heif_context_get_list_of_item_IDs(c,NULL,n));
  heif_context_get_list_of_item_IDs(c,ids,n);ids[n]=0;ids[n+1]=UINT32_MAX;
  for(int i=0;i<n+2;++i) {
    heif_item_id id=ids[i];printf(" I%u=%u/%d/",id,heif_item_get_item_type(c,id),heif_item_is_item_hidden(c,id));
    string(heif_item_get_item_name(c,id));printf("/");string(heif_item_get_mime_item_content_type(c,id));printf("/");string(heif_item_get_mime_item_content_encoding(c,id));printf("/");string(heif_item_get_uri_item_uri_type(c,id));
    payload(c,id);references(c,id);language(c,id);
  }
  free(ids);
}
int main(int argc,char** argv) {
  if(argc!=2 || !(samples=fopen(argv[1],"wb")))return 6;
  uint32_t h[9];
  const char* encodings[]={"","identity","compress_zlib","deflate","br","gzip","unknown","Identity","\xff"};
  while(fread(h,sizeof(h),1,stdin)==1) {
    if(h[3]>10000||h[4]>1000000||h[5]>1000000)return 2;
    char* param=calloc(h[3]+1,1);uint8_t* data=malloc(h[4]+1);uint8_t* file=malloc(h[5]+1);
    if(!param||!data||!file)return 3;
    if(fread(param,1,h[3],stdin)!=h[3]||fread(data,1,h[4],stdin)!=h[4]||fread(file,1,h[5],stdin)!=h[5])return 4;
    struct heif_context* c=heif_context_alloc();printf("C");
    if(h[0]==0)error(heif_context_read_from_memory(c,file,h[5],NULL));
    if(h[7]!=UINT32_MAX)heif_context_get_security_limits(c)->max_memory_block_size=h[7];
    if(h[8]!=UINT32_MAX)heif_context_get_security_limits(c)->max_total_memory=h[8];
    heif_item_id id=0x12345678;const char* arg=h[2]&1?NULL:param;const void* input=h[2]&2?NULL:data;heif_item_id* out=h[2]&4?NULL:&id;
    if(h[0]==1)error(heif_context_add_item(c,arg,input,(int32_t)h[6],out));
    if(h[0]==2)error(heif_context_add_mime_item(c,arg,(enum heif_metadata_compression)h[1],input,(int32_t)h[6],out));
    if(h[0]==3)error(heif_context_add_precompressed_mime_item(c,arg,h[2]&8?NULL:encodings[h[1]%9],input,(int32_t)h[6],out));
    if(h[0]==4)error(heif_context_add_uri_item(c,arg,input,(int32_t)h[6],out));
    printf(" id%u",id);free(param);free(data);free(file);inspect(c);
    heif_item_id first=UINT32_MAX;heif_context_get_list_of_item_IDs(c,&first,1);
    char name[]={'a',(char)0xff,'b',0};error(heif_item_set_item_name(c,first,name));name[0]='z';printf(" name=");string(heif_item_get_item_name(c,first));
    uint32_t prop=999;error(heif_item_set_property_extended_language(c,first,"en-GB",&prop));printf(" p%u",prop);language(c,first);error(heif_item_set_property_extended_language(c,first,"fr",NULL));
    error(heif_context_add_item(c,"zzzz","xyz",3,&id));printf(" next%u",id);error(heif_context_add_item(c,"zzzz",NULL,0,NULL));printf(" count%d",heif_context_get_number_of_items(c));
    /* Native reference insertion requires an initialized meta box. The writer
     * calls above establish it even after an early read failure. */
    heif_item_id to[]={0,2,2,UINT32_MAX};error(heif_context_add_item_reference(c,0x64696d67,first,2));error(heif_context_add_item_references(c,0x64696d67,first,to,4));error(heif_context_add_item_references(c,0x74686d62,first,to,0));references(c,first);
    uint8_t* owned=NULL;size_t size=0;enum heif_metadata_compression method;error(heif_item_get_item_data(c,id,&method,&owned,&size));
    char* lang=NULL;struct heif_error e=heif_item_get_property_extended_language(c,first,&lang);error(e);
    error(heif_context_read_from_memory(c,"x",1,NULL));printf(" reload%d",heif_context_get_number_of_items(c));payload(c,id);
    heif_context_free(c);if(owned){bytes(owned,size);printf(" live%zu/%016llx",size,(unsigned long long)hash(owned,size));}if(lang){printf(" live-lang");string(lang);heif_string_release(lang);}heif_release_item_data(NULL,&owned);
    heif_release_item_data(NULL,NULL);heif_release_item_references(NULL,NULL);heif_string_release(NULL);
    printf("\n");
  }
  struct heif_context* c=heif_context_alloc();printf("NULL");uint32_t id=123;char* lang=(void*)(uintptr_t)0x1234;
  error(heif_item_get_property_extended_language(NULL,1,&lang));error(heif_item_get_property_extended_language(c,1,NULL));error(heif_item_set_property_extended_language(NULL,1,"en",&id));error(heif_item_set_property_extended_language(c,1,NULL,&id));
  printf(" %u/%d",id,lang==(void*)(uintptr_t)0x1234);
  for(int method=-2;method<10;++method)printf(" m%d=%d",method,heif_metadata_compression_method_supported((enum heif_metadata_compression)method));
  heif_context_free(c);printf("\n");if(fclose(samples))return 7;return ferror(stdin)?5:0;
}
