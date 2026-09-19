/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main unused_items_main
#include "items.c"
#undef main
#include <libheif/heif_text.h>
static void text_query(heif_text_item* t){printf(" text%u/",heif_text_item_get_id(t));const char* c=heif_text_item_get_content(t);string(c);heif_string_release(c);char* language=(void*)0x1234;heif_error e=heif_text_item_get_property_extended_language(t,&language);error(e);printf(" lang%d/",language==(void*)0x1234);if(!e.code){string(language);heif_string_release(language);}error(heif_text_item_get_property_extended_language(t,NULL));}
static heif_text_item* lookup(heif_context* ctx,uint32_t id){heif_text_item* t=(void*)0x1234;heif_error e=heif_context_get_text_item(ctx,id,&t);error(e);printf(" get%u/%d",id,t==(void*)0x1234);if(e.code)return NULL;text_query(t);return t;}
static void snapshot(heif_context* ctx,heif_image_handle* h){
 if(h){printf(" metadata/");string(heif_image_handle_get_metadata_content_type(h,3));printf("/");string(heif_image_handle_get_metadata_item_uri_type(h,3));int n=heif_image_handle_get_number_of_text_items(h);printf(" count%d",n);for(int count=0;count<6;count++){uint32_t ids[8];for(unsigned k=0;k<8;k++)ids[k]=0xabcdef;int used=heif_image_handle_get_list_of_text_item_ids(h,ids+1,count);printf(" list%d/%d",count,used);for(unsigned k=0;k<8;k++)printf("/%u",ids[k]);}printf(" null%d",heif_image_handle_get_list_of_text_item_ids(h,NULL,0));}
 for(uint32_t id=0;id<10;id++){heif_text_item* t=lookup(ctx,id);heif_text_item_release(t);}error(heif_context_get_text_item(ctx,1,NULL));
}
int main(int argc,char** argv){if(argc!=2||!(samples=fopen(argv[1],"wb")))return 1;uint32_t v[8];unsigned n=0;while(fread(v,sizeof(v),1,stdin)==1){
 uint8_t* file=malloc(v[0]+1);char* text=calloc(v[1]+1,1);char* type=calloc(v[2]+1,1);char* language=calloc(v[3]+1,1);
 if(fread(file,1,v[0],stdin)!=v[0]||fread(text,1,v[1],stdin)!=v[1]||fread(type,1,v[2],stdin)!=v[2]||fread(language,1,v[3],stdin)!=v[3])return 2;
 uint8_t* reload=malloc(v[7]+1);if(fread(reload,1,v[7],stdin)!=v[7])return 2;
 printf("case%u",n++);heif_context* ctx=heif_context_alloc();if(v[4]&8)heif_context_get_security_limits(ctx)->max_memory_block_size=v[5];if(v[4]&16)heif_context_get_security_limits(ctx)->max_total_memory=v[6];error(heif_context_read_from_memory(ctx,file,v[0],NULL));heif_image_handle* h=NULL;error(heif_context_get_primary_image_handle(ctx,&h));snapshot(ctx,h);
 heif_text_item* first=lookup(ctx,3);heif_text_item* added=NULL;
 if(v[4]&2){error(heif_context_read_from_memory(ctx,"invalid",7,NULL));snapshot(ctx,h);}
 if(h){heif_text_item* out=(void*)0x1234;heif_error e=heif_image_handle_add_text_item(h,type,text,v[4]&1?NULL:&out);error(e);printf(" out%d",out==(void*)0x1234);if(!e.code&&!(v[4]&1))added=out;}
 memset(text,0xff,v[1]);memset(type,0xff,v[2]);free(text);free(type);snapshot(ctx,h);inspect(ctx);
 if(added){text_query(added);uint32_t prop=999;error(heif_text_item_set_extended_language(added,language,v[4]&4?NULL:&prop));printf(" prop%u",prop);text_query(added);error(heif_text_item_set_extended_language(added,"fr",NULL));error(heif_text_item_set_extended_language(added,NULL,&prop));}
 if(first){text_query(first);uint32_t prop=999;error(heif_text_item_set_extended_language(first,language,&prop));printf(" firstprop%u",prop);}
 // A successful reload keeps earlier text objects in the lookup registry.
 error(heif_context_read_from_memory(ctx,reload,v[7],NULL));free(reload);snapshot(ctx,h);heif_text_item* reloaded=lookup(ctx,3);heif_text_item_release(reloaded);free(file);free(language);heif_context_free(ctx);if(h)heif_image_handle_release(h);if(first)text_query(first);if(added)text_query(added);heif_text_item_release(first);heif_text_item_release(added);puts("");
 }
 printf("null");text_query(NULL);uint32_t id=999;error(heif_text_item_set_extended_language(NULL,"en",&id));error(heif_context_get_text_item(NULL,1,NULL));printf(" id%u",id);heif_text_item_release(NULL);puts("");return fclose(samples)?3:0;}
