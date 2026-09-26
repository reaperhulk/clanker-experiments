/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Reuse the independent generic-item snapshots, including full payload bytes. */
#define main unused_items_main
#include "items.c"
#undef main
static void handle_metadata(heif_image_handle* handle){
 const char* filters[]={NULL,"Exif","mime","uri ","zzzz"};uint32_t ids[32];
 for(unsigned k=0;k<5;k++){int n=heif_image_handle_get_number_of_metadata_blocks(handle,filters[k]);printf(" metadata%d",n);int count=heif_image_handle_get_list_of_metadata_block_IDs(handle,filters[k],ids,32);printf("/%d",count);for(int i=0;i<count;i++){printf("/%u/",ids[i]);string(heif_image_handle_get_metadata_type(handle,ids[i]));printf("/");string(heif_image_handle_get_metadata_content_type(handle,ids[i]));printf("/");string(heif_image_handle_get_metadata_item_uri_type(handle,ids[i]));size_t size=heif_image_handle_get_metadata_size(handle,ids[i]);uint8_t* data=malloc(size?size:1);error(heif_image_handle_get_metadata(handle,ids[i],data));bytes(data,size);printf("/%zu/%016llx",size,(unsigned long long)hash(data,size));free(data);}}
}
int main(int argc,char** argv){
 if(argc!=2||!(samples=fopen(argv[1],"wb")))return 1;
 uint32_t v[9];unsigned n=0;while(fread(v,sizeof(v),1,stdin)==1){
  uint8_t* data=malloc(v[3]+1);char* kind=calloc(v[4]+1,1);char* content=calloc(v[5]+1,1);uint8_t* file=malloc(v[6]+1);
  if(fread(data,1,v[3],stdin)!=v[3]||fread(kind,1,v[4],stdin)!=v[4]||fread(content,1,v[5],stdin)!=v[5]||fread(file,1,v[6],stdin)!=v[6])return 2;
  printf("case%u",n++);heif_context* original=heif_context_alloc();error(heif_context_read_from_memory(original,file,v[6],NULL));heif_image_handle* handle=NULL;error(heif_context_get_primary_image_handle(original,&handle));if(!handle)return 3;
  heif_context* ctx=(v[2]&1)?heif_context_alloc():original;
  if(v[2]&2)error(heif_context_read_from_memory(ctx,"invalid",7,NULL));
  if(v[2]&4)heif_context_get_security_limits(ctx)->max_memory_block_size=v[7];
  if(v[2]&8)heif_context_get_security_limits(ctx)->max_total_memory=v[8];
  handle_metadata(handle);uint32_t id=999;const char* type=(v[2]&16)?NULL:kind;const char* ct=(v[2]&32)?NULL:content;void* d=v[3]?data:NULL;
  if(v[0]==0)error(heif_context_add_exif_metadata(ctx,handle,d,v[3]));
  if(v[0]==1)error(heif_context_add_XMP_metadata(ctx,handle,d,v[3]));
  if(v[0]==2)error(heif_context_add_XMP_metadata2(ctx,handle,d,v[3],(heif_metadata_compression)v[1]));
  if(v[0]==3)error(heif_context_add_generic_metadata(ctx,handle,d,v[3],type,ct));
  if(v[0]==4)error(heif_context_add_generic_uri_metadata(ctx,handle,d,v[3],ct,(v[2]&64)?NULL:&id));
  printf(" new%u",id);memset(data,0xff,v[3]);memset(kind,0xff,v[4]);memset(content,0xff,v[5]);free(data);free(kind);free(content);free(file);inspect(ctx);handle_metadata(handle);
  uint32_t next=999;error(heif_context_add_item(ctx,"last","end",3,&next));printf(" next%u",next);
  heif_item_id ids[32];int count=heif_context_get_list_of_item_IDs(ctx,ids,32);uint8_t* owned=NULL;size_t size=0;heif_metadata_compression method;
  if(count>1)error(heif_item_get_item_data(ctx,ids[count-2],&method,&owned,&size));
  heif_context_free(ctx);if(ctx!=original)heif_context_free(original);handle_metadata(handle);heif_image_handle_release(handle);if(owned){bytes(owned,size);printf(" live%zu/%016llx",size,(unsigned long long)hash(owned,size));}heif_release_item_data(NULL,&owned);puts("");
 }
 // Invalid generic type is rejected before either context or handle is read.
 printf("null");error(heif_context_add_generic_metadata(NULL,NULL,NULL,0,NULL,NULL));error(heif_context_add_generic_metadata(NULL,NULL,NULL,0,"abc",NULL));puts("");return fclose(samples)?4:0;
}
