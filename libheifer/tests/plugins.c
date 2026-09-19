/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_plugin.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static uint32_t v[12];
static const heif_encoder_parameter* parameters[10];
static int valid[]={-9,0,7,100};static const char* strings[]={"yes","no",NULL};
static heif_error result(void){return (heif_error){(int32_t)v[8],73,"callback"};}
static void error(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static const char* name_a(void){return "probe A";}static const char* name_b(void){return "probe B";}
static void ei(void){printf(" Ei%d",heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0));}
static void ec(void){int n=heif_get_decoder_descriptors((heif_compression_format)99,NULL,0);printf(" Ec%d,%d",heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0),n);}
static void di(void){printf(" Di%d",heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0));}
static void dc(void){printf(" Dc%d",heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0));}
static heif_error allocate(void** p){printf(" new");*p=NULL;if(v[9]!=2){int* q=malloc(sizeof(int));if(!q)abort();*q=237;*p=q;}return v[9]?(heif_error){2,77,"allocation"}:(heif_error){0,0,"allocated"};}
static void release(void* p){printf(" free%d",*(int*)p);free(p);}
static heif_error quality(void* p,int x){printf(" quality%d,%d",*(int*)p,x);return result();}
static heif_error lossless(void* p,int x){printf(" lossless%d,%d",*(int*)p,x);return result();}
static heif_error logging(void* p,int x){printf(" logging%d,%d",*(int*)p,x);return result();}
static const heif_encoder_parameter** list(void* p){printf(" list%d",*(int*)p);return parameters;}
static heif_error seti(void* p,const char* n,int x){printf(" si%s,%d",n,x);*(int*)p=x;return result();}
static heif_error setb(void* p,const char* n,int x){printf(" sb%s,%d",n,x);*(int*)p=x;return result();}
static heif_error geti(void* p,const char* n,int* x){printf(" gi%s",n);if(x)*x=*(int*)p;return result();}
static heif_error getb(void* p,const char* n,int* x){printf(" gb%s",n);if(x)*x=*(int*)p;return result();}
static heif_error sets(void* p,const char* n,const char* x){printf(" ss%s,%s",n,x);*(int*)p=(int)strlen(x);return result();}
static heif_error gets(void* p,const char* n,char* out,int size){printf(" gs%s,%d,%d",n,size,*(int*)p);if(size>0)snprintf(out,(size_t)size,"returned:%s",n);return result();}
static int priority(unsigned i,heif_compression_format f){printf(" dp%u,%d",i,f);return (int)f==99?(int32_t)v[2+i]:0;}
static int dp0(heif_compression_format f){return priority(0,f);}static int dp1(heif_compression_format f){return priority(1,f);}static int dp2(heif_compression_format f){return priority(2,f);}
static void descriptor(const heif_encoder_descriptor* d){printf(" desc%s,%s,%d,%d,%d,%d,%d",heif_encoder_descriptor_get_name(d),heif_encoder_descriptor_get_id_name(d),heif_encoder_descriptor_get_compression_format(d),heif_encoder_descriptor_supports_lossy_compression(d),heif_encoder_descriptor_supports_lossless_compression(d),heif_encoder_descriptor_supportes_lossy_compression(d),heif_encoder_descriptor_supportes_lossless_compression(d));}
static void exercise(heif_encoder* e){
 printf(" encoder%s",heif_encoder_get_name(e));const char* names[]={"range","set","bool","str","weird","duplicate","unknown","old"};
 const char* values[]={"", "-9", "7", "100", "true", "false", "1", "TRUE", "  +42tail", "-2147483648", "2147483647", "\t-0"};
 error(heif_encoder_set_lossy_quality(e,(int32_t)v[10]));error(heif_encoder_set_lossless(e,-7));error(heif_encoder_set_logging_level(e,8));
 const heif_encoder_parameter*const* ps=heif_encoder_list_parameters(e);printf(" same%d",ps==parameters);
 for(unsigned i=0;ps[i];i++)printf(" p%s,%d,%d",heif_encoder_parameter_get_name(ps[i]),heif_encoder_parameter_get_type(ps[i]),heif_encoder_has_default(e,heif_encoder_parameter_get_name(ps[i])));
 for(unsigned i=0;i<sizeof(names)/sizeof(names[0]);i++){
  const char* n=names[i];printf(" default%d",heif_encoder_has_default(e,n));int a=11,b=22,c=33,d=44,num=55;const int* ints=valid+1;const char*const* ss=strings+1;
  error(heif_encoder_parameter_integer_valid_range(e,n,NULL,NULL,NULL));error(heif_encoder_parameter_integer_valid_values(e,n,NULL,NULL,NULL,NULL,NULL,NULL));error(heif_encoder_parameter_string_valid_values(e,n,NULL));
  error(heif_encoder_parameter_integer_valid_range(e,n,&a,&b,&c));printf(" r%d,%d,%d",a,b,c);
  error(heif_encoder_parameter_integer_valid_values(e,n,&a,&b,&c,&d,&num,&ints));printf(" v%d,%d,%d,%d,%d,%d,%d",a,b,c,d,num,ints==valid,ints==valid+1);
  error(heif_encoder_parameter_string_valid_values(e,n,&ss));printf(" strings%d,%d",ss==strings,ss==strings+1);
  error(heif_encoder_set_parameter_integer(e,n,(int32_t)v[10]));error(heif_encoder_get_parameter_integer(e,n,&a));printf(" giout%d",a);
  error(heif_encoder_set_parameter_boolean(e,n,(int32_t)v[10]));error(heif_encoder_get_parameter_boolean(e,n,&a));printf(" gbout%d",a);
  error(heif_encoder_set_parameter_string(e,n,"direct"));char buffer[32];memset(buffer,0x5a,sizeof(buffer));error(heif_encoder_get_parameter_string(e,n,buffer,5));printf(" gsout");for(unsigned j=0;j<8;j++)printf("%02x",(unsigned char)buffer[j]);
  for(unsigned j=0;j<sizeof(values)/sizeof(values[0]);j++){error(heif_encoder_set_parameter(e,n,values[j]));memset(buffer,0x5a,sizeof(buffer));int size=(int)(j%8);error(heif_encoder_get_parameter(e,n,size?buffer:NULL,size));printf(" out");for(unsigned k=0;k<10;k++)printf("%02x",(unsigned char)buffer[k]);}
 }
}
#define END(t,f) (offsetof(t,f)+sizeof(((t*)0)->f))
int main(void){while(fread(v,sizeof(v),1,stdin)==1){
 heif_deinit();if(!(v[7]&16)){error(heif_init(NULL));error(heif_init(NULL));}
 const heif_decoder_descriptor* retained_builtin=NULL;
 if(v[7]&8){printf(" builtin-d%d",heif_get_decoder_descriptors(heif_compression_uncompressed,&retained_builtin,1));printf(" %s,%s",heif_decoder_descriptor_get_name(retained_builtin),heif_decoder_descriptor_get_id_name(retained_builtin));for(int format=8;format<=9;format++){const heif_encoder_descriptor* bd[4];int n=heif_get_encoder_descriptors((heif_compression_format)format,NULL,bd,4);printf(" builtin%d",n);for(int i=0;i<n;i++){descriptor(bd[i]);heif_encoder* e=NULL;error(heif_context_get_encoder(NULL,bd[i],&e));exercise(e);heif_encoder_release(e);}}}
 heif_encoder_parameter p[8];memset(p,0,sizeof(p));const char* names[]={"range","set","bool","str","weird","duplicate","duplicate","old"};
 for(unsigned i=0;i<8;i++){p[i].version=i==7?1:(int32_t)v[5];p[i].name=names[i];p[i].type=heif_encoder_parameter_type_integer;p[i].has_default=(int32_t)v[11];}
 p[0].integer.have_minimum_maximum=(uint8_t)v[6];p[0].integer.minimum=-9;p[0].integer.maximum=100;
 p[1].integer.num_valid_values=4;p[1].integer.valid_values=valid;p[2].type=heif_encoder_parameter_type_boolean;p[3].type=heif_encoder_parameter_type_string;p[3].string.valid_values=strings;p[4].type=(heif_encoder_parameter_type)99;
 p[5].integer.have_minimum_maximum=1;p[5].integer.minimum=-9;p[5].integer.maximum=100;p[6].integer.num_valid_values=4;p[6].integer.valid_values=valid;if(v[7]&4)p[6].type=heif_encoder_parameter_type_string;
 for(unsigned i=0;i<8;i++){size_t sz=p[i].version<2?offsetof(heif_encoder_parameter,has_default):sizeof(p[i]);void* q=malloc(sz);memcpy(q,p+i,sz);parameters[i]=q;}parameters[8]=NULL;
 heif_encoder_plugin ep={0};ep.plugin_api_version=(int32_t)v[0];ep.compression_format=(heif_compression_format)99;ep.id_name="probe-a";ep.priority=(int32_t)v[2];ep.supports_lossy_compression=-17;ep.supports_lossless_compression=9;ep.get_plugin_name=name_a;ep.init_plugin=v[7]&1?NULL:ei;ep.cleanup_plugin=v[7]&1?NULL:ec;ep.new_encoder=allocate;ep.free_encoder=release;ep.set_parameter_quality=quality;ep.set_parameter_lossless=lossless;ep.set_parameter_logging_level=v[7]&2?NULL:logging;ep.list_parameters=list;ep.set_parameter_integer=seti;ep.get_parameter_integer=geti;ep.set_parameter_boolean=setb;ep.get_parameter_boolean=getb;ep.set_parameter_string=sets;ep.get_parameter_string=gets;
 size_t en=ep.plugin_api_version<=1?END(heif_encoder_plugin,get_compressed_data):ep.plugin_api_version==2?END(heif_encoder_plugin,query_input_colorspace2):ep.plugin_api_version==3?END(heif_encoder_plugin,query_encoded_size):sizeof(ep);
 void* ea=malloc(en);void* eb=malloc(en);memcpy(ea,&ep,en);ep.id_name="probe-b";ep.get_plugin_name=name_b;ep.priority=(int32_t)v[3];memcpy(eb,&ep,en);
 error(heif_register_encoder_plugin(ea));error(heif_register_encoder_plugin(eb));error(heif_register_encoder_plugin(ea));
 heif_decoder_plugin dp[3];memset(dp,0,sizeof(dp));size_t dn=(int32_t)v[1]<=1?END(heif_decoder_plugin,decode_image):(int32_t)v[1]==2?END(heif_decoder_plugin,set_strict_decoding):(int32_t)v[1]==3?END(heif_decoder_plugin,id_name):(int32_t)v[1]==4?END(heif_decoder_plugin,decode_next_image):sizeof(dp[0]);
 /* Separate exact historical allocations; order by their actual address for a stable trace. */
 void* ds[3];for(unsigned i=0;i<3;i++)ds[i]=malloc(dn);for(unsigned i=0;i<3;i++)for(unsigned j=i+1;j<3;j++)if((uintptr_t)ds[i]>(uintptr_t)ds[j]){void* q=ds[i];ds[i]=ds[j];ds[j]=q;}
 for(unsigned i=0;i<3;i++){dp[i].plugin_api_version=(int32_t)v[1];dp[i].get_plugin_name=i==1?name_b:name_a;dp[i].id_name=i==1?"decode-b":"decode-a";dp[i].init_plugin=v[7]&1?NULL:di;dp[i].deinit_plugin=v[7]&1?NULL:dc;dp[i].does_support_format=i==0?dp0:i==1?dp1:dp2;memcpy(ds[i],dp+i,dn);error(heif_register_decoder_plugin(ds[i]));}error(heif_register_decoder(NULL,ds[0]));
 const heif_encoder_descriptor* ed[8];int caps[]={-3,0,1,2,3,8};const char* filters[]={NULL,"probe-a","probe-b","probe",""};
 for(unsigned f=0;f<5;f++)for(unsigned j=0;j<6;j++){for(unsigned k=0;k<8;k++)ed[k]=NULL;int n=heif_get_encoder_descriptors((heif_compression_format)99,filters[f],ed,caps[j]);printf(" enc%d,%d",n,ed[n>0?n:0]==NULL);for(int k=0;k<n;k++)descriptor(ed[k]);printf(" count%d",heif_context_get_encoder_descriptors(NULL,(heif_compression_format)99,filters[f],NULL,caps[j]));}
 printf(" have%d,%d",heif_have_encoder_for_format((heif_compression_format)99),heif_have_encoder_for_format((heif_compression_format)-1));
 for(unsigned j=0;j<6;j++){const heif_decoder_descriptor* dd[8]={0};int n=heif_get_decoder_descriptors((heif_compression_format)99,dd,caps[j]);printf(" dec%d",n);for(int k=0;k<n;k++){const char* id=heif_decoder_descriptor_get_id_name(dd[k]);printf(" d%s,%s,%d,%d,%d",heif_decoder_descriptor_get_name(dd[k]),id?id:"NULL",(const void*)dd[k]==ds[0],(const void*)dd[k]==ds[1],(const void*)dd[k]==ds[2]);}int all=heif_get_decoder_descriptors((heif_compression_format)99,NULL,caps[j]);printf(" dc%d",all);}
 int have=heif_have_decoder_for_format((heif_compression_format)99);printf(" dh%d",have);(void)heif_get_decoder_descriptors(heif_compression_undefined,NULL,0);
 int n=heif_get_encoder_descriptors((heif_compression_format)99,"probe-a",ed,8);if(n){heif_encoder* e=NULL;error(heif_context_get_encoder(NULL,ed[0],&e));printf(" allocated%d",e!=NULL);if(v[9]!=2)exercise(e);heif_encoder_release(e);}
 heif_context* ctx=heif_context_alloc();heif_encoder* ce=NULL;error(heif_context_get_encoder_for_format(ctx,(heif_compression_format)-1,&ce));heif_context_free(ctx);
 heif_encoder* e=NULL;error(heif_context_get_encoder_for_format(NULL,(heif_compression_format)99,&e));if(e){printf(" chosen%s",heif_encoder_get_name(e));heif_encoder_release(e);}
 error(heif_context_get_encoder_for_format(NULL,(heif_compression_format)-1,&e));printf(" absent%d",e==NULL);
 error(heif_context_get_encoder(NULL,NULL,&e));error(heif_context_get_encoder_for_format(NULL,(heif_compression_format)99,NULL));error(heif_register_encoder_plugin(NULL));error(heif_register_decoder_plugin(NULL));error(heif_encoder_set_lossy_quality(NULL,1));error(heif_encoder_set_lossless(NULL,1));error(heif_encoder_set_logging_level(NULL,1));heif_encoder_release(NULL);
 heif_deinit();printf(" retained%d",heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0));heif_deinit();printf(" removed%d",heif_get_encoder_descriptors((heif_compression_format)99,NULL,NULL,0));heif_deinit();heif_deinit();if(retained_builtin)printf(" retained-builtin%s,%s",heif_decoder_descriptor_get_name(retained_builtin),heif_decoder_descriptor_get_id_name(retained_builtin));
 for(unsigned i=0;i<8;i++)free((void*)parameters[i]);for(unsigned i=0;i<3;i++)free(ds[i]);free(ea);free(eb);puts("");
 }return 0;}
