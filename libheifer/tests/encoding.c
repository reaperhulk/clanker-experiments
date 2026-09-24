/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_items.h>
#include <libheif/heif_properties.h>
#include <libheif/heif_uncompressed.h>
#include <libheif/heif_regions.h>
#include <libheif/heif_text.h>
#include <libheif/heif_aux_images.h>
#include <libheif/heif_sequences.h>
#include <libheif/heif_omaf.h>
#include <libheif/heif_tai_timestamps.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e) { printf(" e%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL"); }
static void inspect(heif_image_handle* h,int full) {
  if(!full) { printf(" encoded-size%d,%d,%d,%d,%d",heif_image_handle_get_width(h),heif_image_handle_get_height(h),heif_image_handle_get_ispe_width(h),heif_image_handle_get_ispe_height(h),heif_image_handle_is_primary_image(h)); return; }
  printf(" handle%d,%d,%d,%d,%d,%d,%d,%d",heif_image_handle_get_width(h),heif_image_handle_get_height(h),heif_image_handle_get_ispe_width(h),heif_image_handle_get_ispe_height(h),heif_image_handle_is_primary_image(h),heif_image_handle_get_luma_bits_per_pixel(h),heif_image_handle_get_chroma_bits_per_pixel(h),heif_image_handle_has_alpha_channel(h));
  heif_image* dec=NULL; error(heif_decode_image(h,&dec,heif_colorspace_undefined,heif_chroma_undefined,NULL));
  if(dec) {
    for(int ch=0;ch<=10;ch++) {
      if(!heif_image_has_channel(dec,(heif_channel)ch)) continue;
      int w=heif_image_get_width(dec,(heif_channel)ch),ht=heif_image_get_height(dec,(heif_channel)ch),stride=0,bits=heif_image_get_bits_per_pixel(dec,(heif_channel)ch);
      const uint8_t* p=heif_image_get_plane_readonly(dec,(heif_channel)ch,&stride);
      printf(" pixels%d,%d,%d,%d,%d:",ch,w,ht,bits,heif_image_get_bits_per_pixel_range(dec,(heif_channel)ch));
      if(p) for(int y=0;y<ht;y++) for(int x=0;x<w*((bits+7)/8);x++) printf("%02x",p[y*stride+x]);
    }
    heif_image_release(dec);
  }
}
static heif_error output(heif_context* ctx,const void* data,size_t size,void* u) {
  (void)ctx; (void)u;
  printf(" file%zu:",size); for(size_t i=0;i<size;i++) printf("%02x",((const uint8_t*)data)[i]);
  heif_context* read=heif_context_alloc(); heif_error e=heif_context_read_from_memory(read,data,size,NULL); error(e);
  if(e.code==0) { heif_image_handle* handle=NULL; error(heif_context_get_primary_image_handle(read,&handle)); if(handle) { inspect(handle,1); heif_image_handle_release(handle); } }
  heif_context_free(read); return (heif_error){0,0,"Success"};
}
/* Encoder parameter sets selected by v[0] bits 24-31 (1-based): name=value pairs separated by ';'. */
static const char* const parameter_sets[]={
  "progression_order=LRCP","progression_order=RLCP","progression_order=RPCL","progression_order=PCRL","progression_order=CPRL",
  "num_decompositions=0","num_decompositions=1","num_decompositions=3","num_decompositions=6","num_decompositions=32",
  "tile_size=32,24","tile_size=16,16;tilepart_division=resolution","tile_size=16,16;tilepart_division=component",
  "tile_size=16,16;tilepart_division=both;tlm_marker=true","tlm_marker=true","tilepart_division=both",
  "block_dimensions=4,4","block_dimensions=8,128","block_dimensions=1024,4","block_dimensions=16,32",
  "codestream_comment=libheifer","codestream_comment=","tile_size=24,40;progression_order=PCRL;block_dimensions=8,8",
  "tile_size=7,5;num_decompositions=2","num_decompositions=33","block_dimensions=2048,2","block_dimensions=64",
  "progression_order=XXXX","tilepart_division=bad","tile_size=0,5","chroma=422","lossless=true",
  "tile_size=32,24;tilepart_division=both;progression_order=CPRL;num_decompositions=2",
  "tile_size=16,16;progression_order=RLCP;tilepart_division=resolution;tlm_marker=true",
  "tile_size=20,12;progression_order=RPCL;tilepart_division=component","num_decompositions=5;block_dimensions=32,32;tile_size=48,48",
  "tile_size=64,64;tilepart_division=both;progression_order=LRCP","tile_size=5,64;num_decompositions=4;block_dimensions=4,64",
  "codestream_comment=a much longer comment string for the COM marker;tlm_marker=true",
};
static void set_parameters(heif_encoder* enc,unsigned index) {
  if(!enc||!index||index>sizeof(parameter_sets)/sizeof(*parameter_sets)) return;
  char buffer[256]; strncpy(buffer,parameter_sets[index-1],sizeof(buffer)-1); buffer[sizeof(buffer)-1]=0;
  for(char* item=strtok(buffer,";");item;item=strtok(NULL,";")) {
    char* eq=strchr(item,'='); if(!eq) continue; *eq=0; printf(" param%s",item); error(heif_encoder_set_parameter(enc,item,eq+1));
  }
}
int main(void) {
  setvbuf(stdout,NULL,_IONBF,0); uint32_t v[8];
  while(fread(v,sizeof(v),1,stdin)==1) {
    heif_context* ctx=heif_context_alloc(); heif_encoder* enc=NULL;
    error(heif_context_get_encoder_for_format(ctx,(heif_compression_format)(v[0]&255),&enc));
    /* v[0] bits 16-23: lossy quality + 1; bit 14: lossless; bit 15: not lossless; bit 11: chroma "420". */
    if(enc&&(v[0]>>16)) error(heif_encoder_set_lossy_quality(enc,(int)((v[0]>>16)&255)-1));
    if(enc&&(v[0]&16384)) error(heif_encoder_set_lossless(enc,1));
    if(enc&&(v[0]&32768)) error(heif_encoder_set_lossless(enc,0));
    if(enc&&(v[0]&2048)) error(heif_encoder_set_parameter_string(enc,"chroma","420"));
    set_parameters(enc,v[0]>>24);
    heif_image* img=NULL; error(heif_image_create(v[1],v[2],(heif_colorspace)v[3],(heif_chroma)v[4],&img));
    if(img&&v[5]) {
      int channels[4]={0,1,2,6}; int count=v[3]==2?1:3;
      if(v[3]==1) { channels[0]=3; channels[1]=4; channels[2]=5; if(v[4]>=10) { channels[0]=10; count=1; } }
      if(v[7]&512) channels[count++]=6;
      for(int i=0;i<count;i++) {
        int ch=channels[i],w=v[1],height=v[2];
        if((ch==1||ch==2)&&(v[4]==1||v[4]==2)) { w=(w+1)/2; if(v[4]==1) height=(height+1)/2; }
        error(heif_image_add_plane(img,(heif_channel)ch,w,height,v[5]));
        int stride=0; uint8_t* p=heif_image_get_plane(img,(heif_channel)ch,&stride); int bytes=v[5]>8?2:1;
        if(ch==10) bytes*=v[4]==11||v[4]==13||v[4]==15?4:3;
        if(p) for(int y=0;y<height;y++) for(int x=0;x<w*bytes;x++) p[y*stride+x]=(uint8_t)(x*19+y*7+i*53);
      }
    }
    if(img&&(v[7]&1024)) {
      heif_image_set_pixel_aspect_ratio(img,3,2); heif_content_light_level clli={123,45}; heif_image_set_content_light_level(img,&clli);
      heif_mastering_display_colour_volume mdcv={{1,2,3},{4,5,6},7,8,999,100}; heif_image_set_mastering_display_colour_volume(img,&mdcv);
      heif_ambient_viewing_environment amve={500,100,200}; heif_image_set_ambient_viewing_environment(img,&amve); heif_image_set_nominal_diffuse_white_luminance(img,333);
      heif_image_set_gimi_sample_content_id(img,"sample content"); heif_image_set_omaf_image_projection(img,heif_omaf_image_projection_equirectangular);
      heif_color_profile_nclx* n=heif_nclx_color_profile_alloc(); n->matrix_coefficients=6; error(heif_image_set_nclx_color_profile(img,n)); heif_nclx_color_profile_free(n);
    }
    if(img&&(v[7]&0x80000000U)) {
      heif_tai_timestamp_packet* t=heif_tai_timestamp_packet_alloc(); t->tai_timestamp=123456789; t->synchronization_state=1; error(heif_image_set_tai_timestamp(img,t)); heif_tai_timestamp_packet_release(t);
    }
    if(img&&(v[7]&2048)) error(heif_image_set_raw_color_profile(img,"prof","ICC test bytes",14));
    heif_encoding_options* o=heif_encoding_options_alloc(); o->save_two_colr_boxes_when_ICC_and_nclx_available=!!(v[7]&16384); o->macOS_compatibility_workaround_no_nclx_profile=!!(v[7]&32768); o->image_orientation=(heif_orientation)v[6]; o->version=(uint8_t)(v[7]&255);
    heif_unci_image_parameters* params=heif_unci_image_parameters_alloc(); params->compression=(heif_unci_compression)((v[7]>>16)&255); o->unci_parameters=params;
    heif_image_handle* h=(void*)(uintptr_t)1;
    error(heif_context_encode_image(ctx,img,v[0]&8192?NULL:enc,v[7]&256?NULL:o,v[0]&4096?NULL:&h));
    printf(" out%d count%d items%d",h==NULL?0:h==(void*)(uintptr_t)1?1:2,heif_context_get_number_of_top_level_images(ctx),heif_context_get_number_of_items(ctx));
    if(h&&h!=(void*)(uintptr_t)1) {
      inspect(h,(v[0]&255)==9);
      if(v[7]&4096) {
        heif_region_item* r=NULL; error(heif_image_handle_add_region_item(h,v[1],v[2],&r));
        if(r) { error(heif_region_item_add_region_point(r,-17,23,NULL)); error(heif_region_item_add_region_rectangle(r,-3,4,5,6,NULL)); error(heif_region_item_add_region_ellipse(r,8,-9,11,12,NULL)); heif_region_item_release(r); }
        heif_text_item* t=NULL; error(heif_image_handle_add_text_item(h,"text/plain","some text",&t)); if(t) heif_text_item_release(t);
      }
      if(v[7]&8192) { heif_image_handle* second=NULL; error(heif_context_encode_image(ctx,img,enc,o,&second)); if(second) { error(heif_context_set_primary_image(ctx,second)); printf(" primaries%d,%d",heif_image_handle_is_primary_image(h),heif_image_handle_is_primary_image(second)); heif_image_handle_release(second); } }
      heif_item_id id=0; error(heif_context_get_primary_image_ID(ctx,&id)); printf(" primary%u",id);
      if(v[0]&256) {
        heif_item_id ids[]={heif_image_handle_get_item_id(h)}; int32_t offsets[]={-1,2}; uint16_t bg[]={100,200,300,65535}; heif_image_handle* overlay=(void*)(uintptr_t)1;
        error(heif_context_add_overlay_image(ctx,v[1]+1,v[2]+2,1,ids,offsets,bg,&overlay)); printf(" overlay%d",overlay==NULL?0:overlay==(void*)(uintptr_t)1?1:2);
        if(overlay&&overlay!=(void*)(uintptr_t)1) { inspect(overlay,0); error(heif_context_set_primary_image(ctx,overlay)); heif_image_handle_release(overlay); }
      }
      if(v[0]&512) {
        heif_image_handle* thumb=(void*)(uintptr_t)1; error(heif_context_encode_thumbnail(ctx,img,h,enc,o,(int32_t)v[6],&thumb)); printf(" thumb%d",thumb==NULL?0:thumb==(void*)(uintptr_t)1?1:2);
        if(thumb&&thumb!=(void*)(uintptr_t)1) { inspect(thumb,0); printf(" nthumbnails%d",heif_image_handle_get_number_of_thumbnails(h)); heif_image_handle_release(thumb); }
      }
      if(v[0]&1024) { heif_image_handle* second=NULL; error(heif_context_encode_image(ctx,img,enc,o,&second)); if(second) { error(heif_context_assign_thumbnail(ctx,h,second)); heif_image_handle_release(second); } }
      heif_writer writer={1,output}; error(heif_context_write(ctx,&writer,NULL)); if(v[7]&4096) error(heif_context_write(ctx,&writer,NULL)); heif_image_handle_release(h);
    }
    heif_unci_image_parameters_release(params); heif_encoding_options_free(o); if(img) heif_image_release(img); if(enc) heif_encoder_release(enc); heif_context_free(ctx); puts("");
  }
  return 0;
}
