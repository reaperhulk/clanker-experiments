/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_plugin.h>
#include <libheif/heif_sequences.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
/* v[0] plugin version; v[1] format in the low byte, priority above it (0: 777);
   v[2] failing stage (1 new, 2 push, 3 flush, 4 decode) at its v[10]-th call (0: every call),
   with subcode v[3] and a prefixed message when v[4]; v[5] frames held back until the flush;
   v[6..8] output width, height, bits; v[9] flags: 1 no new_decoder, 2 no decode_next_image,
   4 no push_data2, 8 no decode_next_image2, 16 select by id, 32 absent id, 64 wrong user data,
   128 ignore the edit list; v[11] decode calls (0: 10). */
static uint32_t v[12];
typedef struct { uintptr_t queue[256]; unsigned count, flushed; } state;
static unsigned calls[5];
static heif_error ok(void){return (heif_error){0,0,"callback-success"};}
static heif_error failure(unsigned stage){calls[stage]++;return v[2]==stage&&(v[10]==0||calls[stage]==v[10])?(heif_error){7,(int)v[3],v[4]?"Decoder plugin generated an error: Unspecified: detail":"detail"}:ok();}
static void err(heif_error e){printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static const char* name(void){return "callback decoder";}
static int support(heif_compression_format f){return f==(int)(v[1]&255)?(v[1]>>8?(int)(v[1]>>8):777):0;}
static void init(void){printf(" init");}
static void cleanup(void){printf(" cleanup");}
static heif_error allocate(void** out){printf(" old-new");*out=calloc(1,sizeof(state));return failure(1);}
static heif_error allocate2(void** out,const heif_decoder_plugin_options* o){printf(" new%d,%d,%d,%d",o->format,o->strict_decoding,o->num_threads,o->limits!=NULL);*out=calloc(1,sizeof(state));return failure(1);}
static void release(void* p){printf(" free%d",p!=NULL);free(p);}
static void strict(void* p,int x){(void)p;printf(" strict%d",x);}
static heif_error record(void* p,const void* data,size_t size,uintptr_t user){state* s=p;uint32_t h=2166136261u;for(size_t i=0;i<size;i++)h=(h^((const uint8_t*)data)[i])*16777619u;printf(" push%zu:%08x",size,h);if(s->count<256)s->queue[s->count++]=user;return failure(2);}
static heif_error push(void* p,const void* data,size_t size){return record(p,data,size,0);}
static heif_error push2(void* p,const void* data,size_t size,uintptr_t user){printf(" user%zu",(size_t)user);return record(p,data,size,user);}
static heif_error flush(void* p){printf(" flush%d",p!=NULL);if(p)((state*)p)->flushed=1;return failure(3);}
static heif_error next(void* p,heif_image** out,uintptr_t* user){state* s=p;heif_error e=failure(4);if(e.code)return e;if(s->count==0||(!s->flushed&&s->count<=v[5]))return ok();uintptr_t u=s->queue[0];memmove(s->queue,s->queue+1,(--s->count)*sizeof(uintptr_t));if(user)*user=(v[9]&64)?u+100:u;int width=(int)v[6],height=(int)v[7];e=heif_image_create(width,height,heif_colorspace_monochrome,heif_chroma_monochrome,out);if(e.code)return e;e=heif_image_add_plane(*out,heif_channel_Y,width,height,(int)v[8]);if(e.code)abort();int stride;uint8_t* bytes=heif_image_get_plane(*out,heif_channel_Y,&stride);for(int y=0;y<height;y++)for(int x=0;x<width*((v[8]+7)/8);x++)bytes[y*stride+x]=(uint8_t)(y*53+x*31+19+u*7);return ok();}
static heif_error decode(void* p,heif_image** out){printf(" decode");return next(p,out,NULL);}
static heif_error decode1(void* p,heif_image** out,const heif_security_limits* l){(void)l;printf(" next1");return next(p,out,NULL);}
static heif_error decode2(void* p,heif_image** out,uintptr_t* user,const heif_security_limits* l){(void)l;printf(" next2");return next(p,out,user);}
#define END(t,f) (offsetof(t,f)+sizeof(((t*)0)->f))
int main(void){uint32_t n;while(fread(v,sizeof(v),1,stdin)==1&&fread(&n,4,1,stdin)==1){uint8_t* data=malloc(n?n:1);if(fread(data,1,n,stdin)!=n)return 2;memset(calls,0,sizeof(calls));heif_init(NULL);heif_decoder_plugin p={0};p.plugin_api_version=(int)v[0];p.get_plugin_name=name;p.init_plugin=init;p.deinit_plugin=cleanup;p.does_support_format=support;p.new_decoder=(v[9]&1)?NULL:allocate;p.free_decoder=release;p.push_data=push;p.decode_image=decode;p.set_strict_decoding=strict;p.id_name="callback-decoder";p.decode_next_image=(v[9]&2)?NULL:decode1;p.new_decoder2=allocate2;p.push_data2=(v[9]&4)?NULL:push2;p.flush_data=flush;p.decode_next_image2=(v[9]&8)?NULL:decode2;size_t sz=v[0]==1?END(heif_decoder_plugin,decode_image):v[0]==2?END(heif_decoder_plugin,set_strict_decoding):v[0]==3?END(heif_decoder_plugin,id_name):v[0]==4?END(heif_decoder_plugin,decode_next_image):sizeof(p);void* rec=malloc(sz);memcpy(rec,&p,sz);err(heif_register_decoder_plugin(rec));heif_context* ctx=heif_context_alloc();err(heif_context_read_from_memory_without_copy(ctx,data,n,NULL));heif_track* track=heif_context_get_track(ctx,0);printf(" track%d",track!=NULL);if(track){heif_decoding_options* opts=heif_decoding_options_alloc();if(v[9]&16)opts->decoder_id="callback-decoder";if(v[9]&32)opts->decoder_id="absent";opts->ignore_sequence_editlist=(v[9]&128)!=0;unsigned count=v[11]?v[11]:10;for(unsigned j=0;j<count;j++){heif_image* image=(heif_image*)(uintptr_t)1;heif_error e=heif_track_decode_next_image(track,&image,heif_colorspace_undefined,heif_chroma_undefined,opts);printf(" |");err(e);printf(" null%d",image==NULL||image==(heif_image*)(uintptr_t)1);if(!e.code&&image){printf(" image%d,%d,%d,%d,%u",heif_image_get_primary_width(image),heif_image_get_primary_height(image),heif_image_get_colorspace(image),heif_image_get_chroma_format(image),heif_image_get_duration(image));for(int ch=0;ch<=10;ch++){if(!heif_image_has_channel(image,(heif_channel)ch))continue;int stride;const uint8_t* bytes=heif_image_get_plane_readonly(image,(heif_channel)ch,&stride);int w=heif_image_get_width(image,(heif_channel)ch),h=heif_image_get_height(image,(heif_channel)ch),bits=heif_image_get_bits_per_pixel(image,(heif_channel)ch);uint32_t hash=2166136261u;for(int y=0;y<h;y++)for(int x=0;x<w*((bits+7)/8);x++)hash=(hash^bytes[y*stride+x])*16777619u;printf(" plane%d,%d,%d,%d:%08x",ch,w,h,bits,hash);}heif_image_release(image);}}heif_decoding_options_free(opts);heif_track_release(track);}heif_context_free(ctx);heif_deinit();free(rec);free(data);puts("");}return 0;}
