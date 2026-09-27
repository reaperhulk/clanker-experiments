/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_sequences.h>
#include <libheif/heif_uncompressed.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
static heif_color_profile_nclx profile;
static heif_unci_image_parameters params;
static void color(heif_color_conversion_options v){printf(" c%u,%d,%d,%u",v.version,v.preferred_chroma_downsampling_algorithm,v.preferred_chroma_upsampling_algorithm,v.only_use_preferred_chroma_algorithm);}
static void encoding(heif_encoding_options* v){printf(" e%u,%u,%u,%u,%d,%u,%d,%u,%d",v->version,v->save_alpha_channel,v->macOS_compatibility_workaround,v->save_two_colr_boxes_when_ICC_and_nclx_available,v->output_nclx_profile==&profile,v->macOS_compatibility_workaround_no_nclx_profile,v->image_orientation,v->prefer_uncC_short_form,v->unci_parameters==&params);printf(" null-profile%d",v->output_nclx_profile==NULL);color(v->color_conversion_options);}
static void sequence(heif_sequence_encoding_options* v){printf(" s%u,%d,%d,%d,%d,%d,%d",v->version,v->output_nclx_profile==&profile,v->gop_structure,v->keyframe_distance_min,v->keyframe_distance_max,v->save_alpha_channel,v->content_kind);printf(" null-profile%d",v->output_nclx_profile==NULL);color(v->color_conversion_options);}
static void unci(heif_unci_image_parameters* v){printf(" u%d,%u,%u,%u,%u,%d",v->version,v->image_width,v->image_height,v->tile_width,v->tile_height,v->compression);}
#define END(t,f) (offsetof(t,f)+sizeof(((t*)0)->f))
static size_t enc_size(unsigned v){size_t sizes[]={1,END(heif_encoding_options,save_alpha_channel),END(heif_encoding_options,macOS_compatibility_workaround),END(heif_encoding_options,save_two_colr_boxes_when_ICC_and_nclx_available),END(heif_encoding_options,macOS_compatibility_workaround_no_nclx_profile),END(heif_encoding_options,image_orientation),END(heif_encoding_options,color_conversion_options),END(heif_encoding_options,prefer_uncC_short_form),END(heif_encoding_options,unci_parameters)};return v<=8?sizes[v]:sizeof(heif_encoding_options);}
static size_t seq_size(unsigned v){size_t sizes[]={1,END(heif_sequence_encoding_options,color_conversion_options),END(heif_sequence_encoding_options,save_alpha_channel),END(heif_sequence_encoding_options,content_kind)};return v<=3?sizes[v]:sizeof(heif_sequence_encoding_options);}
int main(void){
 uint32_t v[4];
 while(fread(v,sizeof(v),1,stdin)==1){
  heif_encoding_options* e=heif_encoding_options_alloc();heif_sequence_encoding_options* s=heif_sequence_encoding_options_alloc();heif_unci_image_parameters* u=heif_unci_image_parameters_alloc();if(!e||!s||!u)abort();encoding(e);sequence(s);unci(u);
  heif_encoding_options es;heif_sequence_encoding_options ss;heif_unci_image_parameters us;
  memset(&es,(int)v[2],sizeof(es));memset(&ss,(int)v[2],sizeof(ss));memset(&us,(int)v[2],sizeof(us));
  es.version=ss.version=(uint8_t)v[0];us.version=(int32_t)v[0];es.output_nclx_profile=&profile;es.unci_parameters=&params;ss.output_nclx_profile=&profile;
  e->version=s->version=(uint8_t)v[1];u->version=(int32_t)v[1];
  heif_encoding_options_copy(e,&es);heif_sequence_encoding_options_copy(s,&ss);heif_unci_image_parameters_copy(u,&us);encoding(e);sequence(s);unci(u);
  heif_encoding_options_copy(e,NULL);heif_sequence_encoding_options_copy(s,NULL);heif_unci_image_parameters_copy(u,NULL);encoding(e);sequence(s);unci(u);
  heif_encoding_options_copy(e,e);heif_sequence_encoding_options_copy(s,s);heif_unci_image_parameters_copy(u,u);encoding(e);sequence(s);unci(u);
  /* Exact historical prefixes, not full modern allocations with old tags. */
  size_t en=enc_size(es.version),sn=seq_size(ss.version);void* ep=malloc(en);void* sp=malloc(sn);memcpy(ep,&es,en);memcpy(sp,&ss,sn);heif_encoding_options_copy(e,ep);heif_sequence_encoding_options_copy(s,sp);free(ep);free(sp);
  en=enc_size(e->version);sn=seq_size(s->version);ep=malloc(en);sp=malloc(sn);memcpy(ep,e,en);memcpy(sp,s,sn);heif_encoding_options_copy(ep,&es);heif_sequence_encoding_options_copy(sp,&ss);heif_encoding_options epout=*e;heif_sequence_encoding_options spout=*s;memcpy(&epout,ep,en);memcpy(&spout,sp,sn);encoding(&epout);sequence(&spout);free(ep);free(sp);
  size_t un=us.version<=0?sizeof(int):sizeof(us);void* up=malloc(un);memcpy(up,&us,un);heif_unci_image_parameters_copy(u,up);free(up);
  un=u->version<=0?sizeof(int):sizeof(*u);up=malloc(un);memcpy(up,u,un);heif_unci_image_parameters_copy(up,&us);heif_unci_image_parameters uout=*u;memcpy(&uout,up,un);unci(&uout);free(up);
  printf(" orientations");int values[]={-2147483647-1,-1,0,1,2,3,4,5,6,7,8,9,2147483647};for(unsigned j=0;j<sizeof(values)/sizeof(values[0]);j++)printf(",%d,%d",heif_orientation_concat((heif_orientation)v[3],(heif_orientation)values[j]),heif_orientation_concat((heif_orientation)values[j],(heif_orientation)v[3]));
  heif_encoding_options_free(e);heif_sequence_encoding_options_release(s);heif_unci_image_parameters_release(u);puts("");
 }
 heif_encoding_options_free(NULL);heif_sequence_encoding_options_release(NULL);heif_unci_image_parameters_release(NULL);heif_unci_image_parameters_copy(NULL,NULL);puts("null releases");return 0;
}
