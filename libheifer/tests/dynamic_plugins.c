/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include <libheif/heif.h>
#include <libheif/heif_plugin.h>
#include <dlfcn.h>
#include <dirent.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int count(void){return heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0);}
static void error(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static void inventory(void){const heif_encoder_descriptor* ed[12]={0};const heif_decoder_descriptor* dd[12]={0};int ne=heif_get_encoder_descriptors((heif_compression_format)99,NULL,ed,12),nd=heif_get_decoder_descriptors((heif_compression_format)99,dd,12);printf(" inventory%d,%d",ne,nd);for(int i=0;i<ne;i++)printf(" E%s",heif_encoder_descriptor_get_id_name(ed[i]));for(int i=0;i<nd;i++)printf(" D%s",heif_decoder_descriptor_get_id_name(dd[i]));}
static void directories(void){const char* const* a=heif_get_plugin_directories();const char* const* b=heif_get_plugin_directories();printf(" dirs");for(int i=0;a[i];i++)printf(" [%s:%d]",a[i],a[i]!=b[i]);heif_free_plugin_directories(a);for(int i=0;b[i];i++)printf(" {%s}",b[i]);heif_free_plugin_directories(b);}
int main(int argc,char** argv){
 if(argc<10)return 2;int mode=atoi(argv[1]),kind=atoi(argv[3]),version=atoi(argv[4]),repeat=atoi(argv[6]),cap=atoi(argv[7]),flags=atoi(argv[8]);unsigned minimum=(unsigned)strtoul(argv[5],NULL,0);int iv=atoi(argv[9]);
 if(mode==0){directories();puts("");return 0;}
 void* pin=dlopen(argv[2],RTLD_LAZY|RTLD_LOCAL);if(pin){void (*configure)(int,int,unsigned,int,int(*)(void))=dlsym(pin,"configure");if(configure)configure(kind,version,minimum,iv,count);}
 void* pins[32]={0};int npins=0;
 if(mode==2||mode==3){DIR* dir=opendir(argv[2]);if(dir){struct dirent* e;while((e=readdir(dir))&&npins<32){char path[4096];snprintf(path,sizeof(path),"%s/%s",argv[2],e->d_name);void* h=dlopen(path,RTLD_LAZY|RTLD_LOCAL);if(h){pins[npins++]=h;void (*configure)(int,int,unsigned,int,int(*)(void))=dlsym(h,"configure");if(configure)configure(kind,version,minimum,iv,count);}}closedir(dir);}}
 if(mode!=3){error(heif_init(NULL));}
 const heif_plugin_info* loaded[16];for(int i=0;i<16;i++)loaded[i]=(const void*)(uintptr_t)0x1234;
 if(mode==1){for(int i=0;i<repeat;i++){error(heif_load_plugin(argv[2],&loaded[i]));printf(" out%d",loaded[i]!=(const void*)(uintptr_t)0x1234);if(loaded[i]!=(const void*)(uintptr_t)0x1234){printf(" info%d,%d,%d,%d",loaded[i]->version,loaded[i]->type,loaded[i]->internal_handle==NULL,i?loaded[i]==loaded[0]:1);}inventory();}
 for(int i=0;i<repeat+1;i++){error(heif_unload_plugin(loaded[0]));inventory();}}
 else if(mode==5){error(heif_load_plugin(argv[2],&loaded[0]));if(pin){dlclose(pin);pin=NULL;}error(heif_unload_plugin(loaded[0]));inventory();}
 else if(mode==4){error(heif_load_plugin(argv[2],&loaded[0]));error(heif_load_plugin(argv[2],&loaded[1]));inventory();error(heif_unload_plugin(loaded[0]));inventory();error(heif_load_plugin(argv[2],&loaded[2]));inventory();for(int i=0;i<4;i++){error(heif_unload_plugin(loaded[0]));inventory();}}
 else if(mode==2){int n=173;error(heif_load_plugins(flags&1?NULL:argv[2],flags&2?NULL:loaded,flags&4?NULL:&n,cap));printf(" n%d",n);for(int i=0;i<16;i++){if(loaded[i]==(const void*)(uintptr_t)0x1234)printf(" S");else if(!loaded[i])printf(" N");else printf(" P%d,%d",loaded[i]->version,loaded[i]->type);}inventory();}
 else {setenv("LIBHEIF_PLUGIN_PATH",argv[2],1);for(int i=0;i<repeat;i++){error(heif_init(NULL));inventory();}}
 heif_deinit();printf(" deinit");inventory();if(flags&8){heif_deinit();printf(" again");inventory();}for(int i=0;i<npins;i++)dlclose(pins[i]);if(pin)dlclose(pin);puts("");return 0;
}
