/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_plugin.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int values[]={-2147483647-1,-99,0,42,2147483647};
static const char *strings[]={"", "value", "\xff\x80", NULL};
static void error(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static void run(heif_encoder_parameter* p){
 printf(" name%d type%d",heif_encoder_parameter_get_name(p)==p->name,heif_encoder_parameter_get_type(p));
 for(unsigned mask=0;mask<64;mask++){
  int a=123,b=456,c=789,d=321,n=654;const int* iv=&values[1];const char*const* sv=&strings[1];
  error(heif_encoder_parameter_get_valid_integer_range(p,mask&1?&a:NULL,mask&2?&b:NULL,mask&4?&c:NULL));printf(" r%d,%d,%d",a,b,c);
  error(heif_encoder_parameter_get_valid_integer_values(p,mask&1?&a:NULL,mask&2?&b:NULL,mask&4?&c:NULL,mask&8?&d:NULL,mask&16?&n:NULL,mask&32?&iv:NULL));printf(" i%d,%d,%d,%d,%d,%d,%d,%d",a,b,c,d,n,iv==values,iv==&values[1],iv==NULL);
  error(heif_encoder_parameter_get_valid_string_values(p,mask&1?&sv:NULL));printf(" s%d,%d,%d",sv==strings,sv==&strings[1],sv==NULL);
  a=987;error(heif_encoder_parameter_get_valid_integer_range(p,&a,&a,&a));printf(" ar%d",a);
  a=987;error(heif_encoder_parameter_get_valid_integer_values(p,&a,&a,&a,&a,&a,&iv));printf(" ai%d",a);
 }
}
int main(void){uint32_t v[8];while(fread(v,sizeof(v),1,stdin)==1){
 heif_encoder_parameter p;memset(&p,0,sizeof(p));char name[]={ 'x', (char)0xff, 'y', 0 };p.version=(int32_t)v[0];p.name=v[6]&1?NULL:name;p.type=(heif_encoder_parameter_type)v[1];p.has_default=(int32_t)v[7];
 if(p.type==heif_encoder_parameter_type_string){p.string.default_value="default";p.string.valid_values=v[6]&2?NULL:strings;}
 else{p.integer.default_value=765;p.integer.have_minimum_maximum=(uint8_t)v[2];p.integer.minimum=(int32_t)v[3];p.integer.maximum=(int32_t)v[4];p.integer.num_valid_values=(int32_t)v[5];p.integer.valid_values=v[6]&2?NULL:values;}
 size_t bytes=p.version<2?offsetof(heif_encoder_parameter,has_default):sizeof(p);void* old=malloc(bytes);if(!old)abort();memcpy(old,&p,bytes);run(old);name[0]='z';printf(" borrowed%d",p.name==NULL||heif_encoder_parameter_get_name(old)[0]=='z');free(old);puts("");
 }return 0;}
