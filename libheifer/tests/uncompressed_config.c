/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_image_handle.h>
#include <libheif/heif_items.h>
#include <libheif/heif_properties.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){printf("e%d,%d,%s;",e.code,e.subcode,e.message?e.message:"NULL");}
static void string(const char* s){if(!s){printf("NULL;");return;}printf("s");for(size_t i=0;s[i];i++)printf("%02x",(unsigned char)s[i]);putchar(';');}
static const char* snapshot(heif_image_handle* handle){
 uint32_t n=heif_image_handle_get_number_of_cmpd_components(handle);printf("n%u;",n);if(n>1000)abort();const char* saved=NULL;
 for(uint32_t i=0;i<n+2;i++){printf("%u=%u,",i,heif_image_handle_get_cmpd_component_type(handle,i));const char* s=heif_image_handle_get_cmpd_component_type_uri(handle,i);string(s);if(s&&!saved)saved=s;else heif_string_release(s);}
 printf("max=%u;",heif_image_handle_get_cmpd_component_type(handle,UINT32_MAX));const char* s=heif_image_handle_get_cmpd_component_type_uri(handle,UINT32_MAX);string(s);heif_string_release(s);return saved;
}
int main(void){
 uint32_t v[7];while(fread(v,sizeof(v),1,stdin)==1){
  uint8_t* data=malloc(v[0]+v[1]);if(fread(data,1,v[0]+v[1],stdin)!=v[0]+v[1])return 2;
  heif_context* ctx=heif_context_alloc();heif_security_limits* limits=heif_context_get_security_limits(ctx);limits->max_components=v[2];limits->max_iso23001_17_pixel_size_bytes=v[3];limits->max_number_of_tiles=v[4];limits->version=v[5];
  error(heif_context_read_from_memory(ctx,data+v[0],v[1],NULL));heif_image_handle* old=NULL;error(heif_context_get_primary_image_handle(ctx,&old));
  error(heif_context_read_from_memory(ctx,data,v[0],NULL));heif_image_handle* current=NULL;error(heif_context_get_primary_image_handle(ctx,&current));
  const char* saved=snapshot(current);const char* os=snapshot(old);heif_string_release(os);
  uint32_t props[64];int n=heif_item_get_properties_of_type(ctx,1,0,props,64);printf("props%d;",n);for(int k=0;k<n;k++)printf("%x,",heif_item_get_property_type(ctx,1,props[k]));
  if(v[6]){error(heif_context_read_from_memory(ctx,data+v[0],v[1],NULL));const char* s=snapshot(current);heif_string_release(s);}
  heif_context_free(ctx);const char* next=snapshot(current);heif_image_handle_release(old);heif_image_handle_release(current);string(saved);string(next);heif_string_release(saved);heif_string_release(next);free(data);puts("");
 }
 const char* s=snapshot(NULL);heif_string_release(s);puts("");return ferror(stdin)||ferror(stdout)?1:0;
}
