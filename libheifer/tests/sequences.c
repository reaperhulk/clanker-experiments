/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_sequences.h>
#include <libheif/heif_tai_timestamps.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
static void error(heif_error e){printf(" e%d,%d,",e.code,e.subcode);if(e.message)for(const unsigned char*p=(void*)e.message;*p;p++)printf("%02x",*p);else printf("NULL");}
static void string(const char*s){printf(" s%d:",s!=NULL);if(s){for(const unsigned char*p=(void*)s;*p;p++)printf("%02x",*p);heif_string_release(s);}}
static void pixels(heif_image*img){printf(" image%u",heif_image_get_duration(img));for(int ch=0;ch<=10;ch++){if(!heif_image_has_channel(img,(heif_channel)ch))continue;int w=heif_image_get_width(img,(heif_channel)ch),h=heif_image_get_height(img,(heif_channel)ch),stride=0,bits=heif_image_get_bits_per_pixel(img,(heif_channel)ch);const uint8_t*p=heif_image_get_plane_readonly(img,(heif_channel)ch,&stride);printf(" plane%d,%d,%d,%d,%d:",ch,w,h,bits,heif_image_get_bits_per_pixel_range(img,(heif_channel)ch));for(int y=0;y<h;y++)for(int x=0;x<w*((bits+7)/8);x++)printf("%02x",p[y*stride+x]);}string(heif_image_get_gimi_sample_content_id(img));}
static void query(heif_track*t){
 printf(" track%u,%08x,%u,%u,%08x,%d,%d",heif_track_get_id(t),heif_track_get_track_handler_type(t),heif_track_get_timescale(t),heif_track_get_number_of_repetitions(t),heif_track_get_sample_entry_type_of_first_cluster(t),heif_track_has_alpha_channel(t),heif_track_get_auxiliary_info_type(t));
 string(heif_track_get_auxiliary_info_type_urn(t));string(heif_track_get_gimi_track_content_id(t));
 const char*u=(void*)(uintptr_t)1;error(heif_track_get_urim_sample_entry_uri_of_first_cluster(t,&u));if(u!=(void*)(uintptr_t)1)string(u);else printf(" untouched");
 uint16_t w=65534,h=65533;error(heif_track_get_image_resolution(t,&w,&h));printf(" size%u,%u",w,h);error(heif_track_get_image_resolution(t,NULL,NULL));
 int n=heif_track_get_number_of_sample_aux_infos(t);heif_sample_aux_info_type a[8];memset(a,0x55,sizeof(a));heif_track_get_sample_aux_info_types(t,a);printf(" aux%d",n);for(int i=0;i<n+1&&i<8;i++)printf(",%08x,%u",a[i].type,a[i].parameter);
 const heif_tai_clock_info*c=heif_track_get_tai_clock_info_of_first_cluster(t);printf(" clock%d",c!=NULL);if(c)printf(",%u,%llu,%u,%d,%u",c->version,(unsigned long long)c->time_uncertainty,c->clock_resolution,c->clock_drift_rate,c->clock_type);
 size_t nr=heif_track_get_number_of_track_reference_types(t);uint32_t refs[32];memset(refs,0x55,sizeof(refs));heif_track_get_track_reference_types(t,refs);printf(" refs%zu",nr);for(size_t i=0;i<nr;i++){printf("/%08x,%zu",refs[i],heif_track_get_number_of_track_reference_of_type(t,refs[i]));uint32_t ids[32];memset(ids,0x55,sizeof(ids));size_t nn=heif_track_get_references_from_track(t,refs[i],ids);for(size_t j=0;j<nn+1;j++)printf(",%u",ids[j]);}
 for(size_t cap=0;cap<4;cap++){uint32_t ids[4]={99,99,99,99};size_t n=heif_track_find_referring_tracks(t,heif_fourcc('c','d','s','c'),ids,cap);printf(" incoming%zu",n);for(int i=0;i<4;i++)printf(",%u",ids[i]);}
}
static void context(heif_context*c){printf(" context%d,%d,%u,%llu",heif_context_has_sequence(c),heif_context_number_of_sequence_tracks(c),heif_context_get_sequence_timescale(c),(unsigned long long)heif_context_get_sequence_duration(c));uint32_t ids[20];memset(ids,0x55,sizeof(ids));heif_context_get_track_ids(c,ids);int n=heif_context_number_of_sequence_tracks(c);for(int i=0;i<n+1;i++)printf(",%u",ids[i]);heif_track*t=heif_context_get_track(c,0);printf(" default%u",t?heif_track_get_id(t):0);heif_track_release(t);t=heif_context_get_track(c,0xffffffff);printf(" absent%d",t!=NULL);heif_track_release(t);}
/* The pinned Box_URIMetaSampleEntry constructor does not initialize its
 * data_reference_index member. These two indeterminate bytes are NOT parity
 * evidence. Mark their structural positions explicitly; compare all other bytes.
 * Never modify the oracle or make a candidate-dependent comparison decision. */
static uint32_t be32(const uint8_t*p){return ((uint32_t)p[0]<<24)|((uint32_t)p[1]<<16)|((uint32_t)p[2]<<8)|p[3];}
static void undefined_urim(const uint8_t*p,size_t n,uint8_t*skip){
 for(size_t at=0;at+8<=n;){uint32_t size=be32(p+at);if(size<8||size>n-at)return;const uint8_t*k=p+at+4;size_t off=8;
  if(!memcmp(k,"urim",4)&&size>=16){skip[at+14]=skip[at+15]=1;}
  else if(!memcmp(k,"stsd",4))off=16;
  else if(memcmp(k,"moov",4)&&memcmp(k,"trak",4)&&memcmp(k,"mdia",4)&&memcmp(k,"minf",4)&&memcmp(k,"stbl",4)){at+=size;continue;}
  if(size>=off)undefined_urim(p+at+off,size-off,skip+at+off);at+=size;
 }
}
static heif_error output(heif_context*c,const void*d,size_t n,void*u){(void)c;(void)u;printf(" file%zu:",n);uint8_t*skip=calloc(n,1);undefined_urim(d,n,skip);
 /* Duplicate tref errors can corrupt the leading ftyp size during native pointer
  * patching. Recover only a structurally bounded moov/mvhd root to identify the
  * same two undefined urim bytes; every damaged header byte is still compared. */
 for(size_t at=8;at+20<=n;at++){const uint8_t*p=(const uint8_t*)d+at;uint32_t size=be32(p);if(size>=20&&size<=n-at&&!memcmp(p+4,"moov",4)&&!memcmp(p+12,"mvhd",4))undefined_urim(p+8,size-8,skip+at+8);}
for(size_t i=0;i<n;i++){if(skip[i])printf("??");else printf("%02x",((const uint8_t*)d)[i]);}free(skip);
 heif_context*r=heif_context_alloc();heif_error e=heif_context_read_from_memory(r,d,n,NULL);error(e);context(r);
 if(e.code==0){int count=heif_context_number_of_sequence_tracks(r);uint32_t ids[20]={0};heif_context_get_track_ids(r,ids);for(int i=0;i<count;i++){heif_track*t=heif_context_get_track(r,ids[i]);query(t);for(int j=0;j<5;j++){
 if(heif_track_get_track_handler_type(t)!=heif_track_type_metadata){heif_image*img=(void*)(uintptr_t)1;heif_error e=heif_track_decode_next_image(t,&img,heif_colorspace_undefined,heif_chroma_undefined,NULL);error(e);printf(" image-out%d",img==(void*)(uintptr_t)1?1:img?2:0);if(img&&img!=(void*)(uintptr_t)1){pixels(img);heif_image_release(img);}continue;}
 heif_raw_sequence_sample*s=(void*)(uintptr_t)1;heif_error e=heif_track_get_next_raw_sequence_sample(t,&s);error(e);printf(" sample-out%d",s==(void*)(uintptr_t)1?1:s?2:0);if(e.code==0&&s){size_t len=0;const uint8_t*p=heif_raw_sequence_sample_get_data(s,&len);printf(" sample%zu,%u:",len,heif_raw_sequence_sample_get_duration(s));for(size_t k=0;k<len;k++)printf("%02x",p[k]);string(heif_raw_sequence_sample_get_gimi_sample_content_id(s));const heif_tai_timestamp_packet*tp=heif_raw_sequence_sample_get_tai_timestamp(s);printf(" timestamp%d",heif_raw_sequence_sample_has_tai_timestamp(s));if(tp)printf(",%llu,%u,%u,%u",(unsigned long long)tp->tai_timestamp,tp->synchronization_state,tp->timestamp_generation_failure,tp->timestamp_is_modified);heif_raw_sequence_sample_release(s);}}heif_track_release(t);}}
 heif_context_free(r);return (heif_error){0,0,"Success"};}
static void still(heif_context*c){heif_encoder*enc=NULL;error(heif_context_get_encoder_for_format(c,heif_compression_uncompressed,&enc));heif_image*img=NULL;error(heif_image_create(2,2,heif_colorspace_monochrome,heif_chroma_monochrome,&img));error(heif_image_add_plane(img,heif_channel_Y,2,2,8));int stride=0;uint8_t*p=heif_image_get_plane(img,heif_channel_Y,&stride);p[0]=1;p[1]=2;p[stride]=3;p[stride+1]=4;heif_image_handle*h=NULL;error(heif_context_encode_image(c,img,enc,NULL,&h));if(h)heif_image_handle_release(h);heif_image_release(img);heif_encoder_release(enc);}
int main(void){setvbuf(stdout,NULL,_IONBF,0);uint32_t v[8];while(fread(v,sizeof(v),1,stdin)==1){
 heif_context*c=heif_context_alloc();context(c);uint32_t flags=v[6];
 if(flags&1)heif_context_set_sequence_timescale(c,v[4]);heif_context_set_number_of_sequence_repetitions(c,v[5]);
 heif_track_options*o=heif_track_options_alloc();heif_track_options_set_timescale(o,v[3]);heif_track_options_set_interleaved_sample_aux_infos(o,(flags&2)?-17:0);
 heif_tai_clock_info*clock=heif_tai_clock_info_alloc();clock->time_uncertainty=0x123456789abcdefULL;clock->clock_resolution=123;clock->clock_drift_rate=-17;clock->clock_type=2;
 error(heif_track_options_enable_sample_tai_timestamps(o,(flags&4)?NULL:clock,(flags>>3)&3));heif_track_options_enable_sample_gimi_content_ids(o,(flags>>5)&3);heif_track_options_set_gimi_track_id(o,(flags&128)?"track-content-id":NULL);heif_tai_clock_info_release(clock);
 heif_encoder*enc=NULL;if(flags&262144)error(heif_context_get_encoder_for_format(c,heif_compression_uncompressed,&enc));
 if(flags&4194304)still(c);
 heif_track*t[8]={0};for(uint32_t i=0;i<v[1];i++){heif_track_type kind=v[0]==1?heif_track_type_image_sequence:v[0]==2?heif_track_type_video:heif_track_type_auxiliary;
 if(v[0]==0||(flags&256&&i%2))error(heif_context_add_uri_metadata_sequence_track(c,i%2?"":"urn:example:metadata",flags&512?NULL:o,&t[i]));else error(heif_context_add_visual_sequence_track(c,4+i,3+i,kind,flags&512?NULL:o,NULL,&t[i]));}
 heif_track_options_set_timescale(o,1);heif_track_options_set_gimi_track_id(o,"modified options");heif_track_options_release(o);
 for(uint32_t i=0;i<v[1];i++)if(t[i]){if(flags&1024){heif_track_add_reference_to_track(t[i],heif_fourcc('c','d','s','c'),t[(i+1)%v[1]]);heif_track_add_reference_to_track(t[i],heif_fourcc('a','u','x','l'),t[i]);if(v[1]>2&&!(flags&2048))heif_track_add_reference_to_track(t[i],heif_fourcc('c','d','s','c'),t[(i+2)%v[1]]);if(flags&2048)heif_track_add_reference_to_track(t[i],heif_fourcc('c','d','s','c'),t[(i+1)%v[1]]);}query(t[i]);
 for(uint32_t j=0;j<v[2];j++){heif_raw_sequence_sample*s=heif_raw_sequence_sample_alloc();uint8_t bytes[16];for(int k=0;k<16;k++)bytes[k]=(uint8_t)(k*17+j+i);error(heif_raw_sequence_sample_set_data(s,bytes,(flags&4096)?j%3:5));heif_raw_sequence_sample_set_duration(s,(flags&8192&&j==0)?0:v[7]+((flags&16384)?j:0));heif_raw_sequence_sample_set_gimi_sample_content_id(s,(flags&32768&&j%2)?"sample-content":NULL);if(flags&65536&&j%2==0){heif_tai_timestamp_packet*p=heif_tai_timestamp_packet_alloc();p->tai_timestamp=123+j;p->synchronization_state=1;heif_raw_sequence_sample_set_tai_timestamp(s,p);heif_tai_timestamp_packet_release(p);}if(flags&262144){
 heif_image*img=NULL;int rgb=!!(flags&524288),depth=(flags&1048576)?12:8;error(heif_image_create(4,3,rgb?heif_colorspace_RGB:heif_colorspace_monochrome,rgb?heif_chroma_444:heif_chroma_monochrome,&img));
 for(int ch=0;ch<(rgb?3:1);ch++){int channel=rgb?ch+3:0;error(heif_image_add_plane(img,(heif_channel)channel,4,3,depth));int stride=0;uint8_t*p=heif_image_get_plane(img,(heif_channel)channel,&stride);for(int y=0;y<3;y++)for(int x=0;x<4*(depth>8?2:1);x++)p[y*stride+x]=(uint8_t)(x*11+y*7+ch*29+j);}
 heif_image_set_duration(img,heif_raw_sequence_sample_get_duration(s));if(flags&32768&&j%2)heif_image_set_gimi_sample_content_id(img,"sample-content");
 heif_sequence_encoding_options*opt=heif_sequence_encoding_options_alloc();error(heif_track_encode_sequence_image(t[i],img,enc,(flags&2097152)?NULL:opt));heif_sequence_encoding_options_release(opt);heif_image_release(img);
 }else error(heif_track_add_raw_sequence_sample(t[i],s));heif_raw_sequence_sample_release(s);}query(t[i]);
 heif_raw_sequence_sample*s=(void*)(uintptr_t)1;error(heif_track_get_next_raw_sequence_sample(t[i],&s));printf(" raw-out%d",s==(void*)(uintptr_t)1?1:s?2:0);if(s&&s!=(void*)(uintptr_t)1)heif_raw_sequence_sample_release(s);error(heif_track_get_next_raw_sequence_sample(t[i],NULL));}
 if(enc){for(uint32_t i=0;i<v[1];i++)if(t[i]&&v[2]>0)error(heif_track_encode_end_of_sequence(t[i],enc));}
 if(flags&8388608)still(c);
 context(c);heif_writer w={1,output};error(heif_context_write(c,&w,NULL));context(c);if(flags&131072){error(heif_context_write(c,&w,NULL));context(c);}if(enc)heif_encoder_release(enc);heif_context_free(c);for(uint32_t i=0;i<v[1];i++)if(t[i]){query(t[i]);heif_track_release(t[i]);}puts("");}return 0;}
