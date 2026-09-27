#define _POSIX_C_SOURCE 200809L
/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_items.h>
#include <libheif/heif_properties.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static void dump(heif_context* ctx){FILE* f=tmpfile();if(!f)exit(3);heif_context_debug_dump_boxes_to_file(ctx,fileno(f));if(fputc(0xa5,f)==EOF||fflush(f))exit(4);long n=ftell(f);rewind(f);printf(" dump%ld:",n);int b;while((b=fgetc(f))!=EOF)printf("%02x",b);fclose(f);}
static uint32_t v[8];static unsigned calls;static void error(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static heif_error write_data(heif_context* ctx,const void* bytes,size_t size,void* userdata){calls++;printf(" cb%d,%d,%zu:",userdata==v,heif_context_get_number_of_items(ctx),size);const uint8_t* p=bytes;for(size_t i=0;i<size;i++)printf("%02x",p[i]);heif_context* read=heif_context_alloc();error(heif_context_read_from_memory(read,bytes,size,NULL));printf(" read%d",heif_context_get_number_of_items(read));heif_context_free(read);return (heif_error){(int32_t)v[5],(int32_t)v[6],v[7]&1?NULL:"writer result"};}
int main(int argc,char** argv){if(argc!=2)return 2;while(fread(v,sizeof(v),1,stdin)==1){heif_context* ctx=heif_context_alloc();calls=0;heif_context_set_unif(ctx,(int32_t)v[4]);heif_context_set_write_mini_format(ctx,v[7]&2);if(v[0])heif_context_set_major_brand(ctx,v[0]);heif_context_add_compatible_brand(ctx,v[1]);heif_context_add_compatible_brand(ctx,v[1]);
 heif_item_id id=0;for(uint32_t i=0;i<v[2];i++){unsigned char data[17];for(unsigned j=0;j<17;j++)data[j]=(unsigned char)(i*13+j*17);if(i%3==0)error(heif_context_add_mime_item(ctx,"application/data",heif_metadata_compression_off,data,i%18,&id));else if(i%3==1)error(heif_context_add_uri_item(ctx,"urn:test",data,i%18,&id));else error(heif_context_add_item(ctx,"test",data,i%18,&id));if(v[7]&128){uint8_t uuid[16];memset(uuid,(int)i,16);uint8_t raw[4]={(uint8_t)i,(uint8_t)(i>>8),55,66};error(heif_item_add_raw_property(ctx,id,i%2?0x61626364:0x75756964,uuid,raw,4,(int)i,NULL));}printf(" id%u",id);error(heif_item_set_item_name(ctx,id,"name"));if(v[7]&4)error(heif_item_set_property_extended_language(ctx,id,i%2?"fr":"en",NULL));if(v[7]&8){heif_item_id to[]={id,5};error(heif_context_add_item_references(ctx,0x63647363,id,to,v[7]&16?2:1));}}
 if(v[7]&32)error(heif_context_read_from_memory(ctx,"bad",3,NULL));
 dump(ctx);heif_writer writer={(int32_t)v[3],write_data};size_t ws=writer.writer_api_version==1?sizeof(writer):sizeof(int);heif_writer* prefix=malloc(ws);memcpy(prefix,&writer,ws);if(v[7]&64)error(heif_context_write(ctx,NULL,v));else{error(heif_context_write(ctx,prefix,v[7]&256?NULL:v));error(heif_context_write(ctx,prefix,v[7]&256?NULL:v));}free(prefix);dump(ctx);printf(" calls%u",calls);remove(argv[1]);error(heif_context_write_to_file(ctx,argv[1]));FILE* f=fopen(argv[1],"rb");printf(" file%d:",f!=NULL);if(f){int c;while((c=fgetc(f))!=EOF)printf("%02x",c);fclose(f);}char missing[4096];snprintf(missing,sizeof(missing),"%s/no-directory/output.heif",argv[1]);error(heif_context_write_to_file(ctx,missing));dump(ctx);heif_context_free(ctx);puts("");}return 0;}
