/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static FILE* output;
static unsigned starts,progresses,ends,cancels;
static void start(heif_progress_step step,int n,void* p){(void)step;(void)n;(void)p;starts++;}
static void progress(heif_progress_step step,int n,void* p){(void)step;(void)n;(void)p;progresses++;}
static void end(heif_progress_step step,void* p){(void)step;(void)p;ends++;}
static int cancel(void* p){(void)p;cancels++;return 1;}
static void number(uint32_t n){for(int i=0;i<4;i++){fputc(n&255,output);n>>=8;}}
static void error(heif_error e){number(e.code);number(e.subcode);size_t n=e.message?strlen(e.message):0;number(n);if(n)fwrite(e.message,1,n,output);}
static void decode(const heif_image_handle* handle,int mode){
  heif_decoding_options* options=heif_decoding_options_alloc();
  options->output_image_nclx_profile_passthrough=mode!=0;
  options->ignore_transformations=mode==1;
  options->start_progress=start;options->on_progress=progress;options->end_progress=end;
  if(mode==9)options->cancel_decoding=cancel;
  if(mode==10)options->decoder_id="unavailable-decoder";
  if(mode==11)options->strict_decoding=1;
  heif_colorspace cs=heif_colorspace_undefined;heif_chroma ch=heif_chroma_undefined;
  if(mode==3){cs=heif_colorspace_RGB;ch=heif_chroma_interleaved_RGB;}
  if(mode==4){cs=heif_colorspace_RGB;ch=heif_chroma_interleaved_RGBA;}
  if(mode==5){cs=heif_colorspace_monochrome;ch=heif_chroma_monochrome;}
  if(mode==6){cs=heif_colorspace_YCbCr;ch=heif_chroma_444;}
  if(mode==7){cs=heif_colorspace_YCbCr;ch=heif_chroma_422;}
  if(mode==8){cs=heif_colorspace_RGB;ch=heif_chroma_444;}
  if(mode>=12 && mode<=15){cs=heif_colorspace_RGB;ch=(heif_chroma)mode;}
  if(mode==16){cs=heif_colorspace_RGB;ch=heif_chroma_interleaved_RGB;options->color_conversion_options.only_use_preferred_chroma_algorithm=1;}
  if(mode==17){cs=heif_colorspace_YCbCr;ch=heif_chroma_422;options->color_conversion_options.only_use_preferred_chroma_algorithm=1;}
  if(mode==18){cs=heif_colorspace_RGB;ch=heif_chroma_interleaved_RGBA;options->color_conversion_options.preferred_chroma_upsampling_algorithm=heif_chroma_upsampling_nearest_neighbor;options->color_conversion_options.only_use_preferred_chroma_algorithm=1;}
  if(mode>=19 && mode<=22){
    options->output_image_nclx_profile=heif_nclx_color_profile_alloc();
    options->output_image_nclx_profile->matrix_coefficients=mode==19?heif_matrix_coefficients_ITU_R_BT_709_5:heif_matrix_coefficients_ITU_R_BT_601_6;
    options->output_image_nclx_profile->full_range_flag=mode%2;
    options->output_image_nclx_profile->transfer_characteristics=heif_transfer_characteristic_ITU_R_BT_709_5;
    if(mode>=21){cs=heif_colorspace_RGB;ch=heif_chroma_interleaved_RGB;}
  }
  heif_image* image=(void*)(uintptr_t)0x1234;
  starts=progresses=ends=cancels=0;
  heif_error e=heif_decode_image(handle,&image,cs,ch,options);error(e);
  number(image==NULL);number(image==(void*)(uintptr_t)0x1234);
  number(starts);number(progresses);number(ends);number(cancels);
  if(!e.code){
    number(heif_image_get_primary_width(image));number(heif_image_get_primary_height(image));
    number(heif_image_get_colorspace(image));number(heif_image_get_chroma_format(image));
    number(heif_image_is_premultiplied_alpha(image));
    uint32_t h,v;heif_image_get_pixel_aspect_ratio(image,&h,&v);number(h);number(v);
    heif_color_profile_nclx* n=NULL;e=heif_image_get_nclx_color_profile(image,&n);error(e);
    if(!e.code){number(n->version);number(n->color_primaries);number(n->transfer_characteristics);number(n->matrix_coefficients);number(n->full_range_flag);heif_nclx_color_profile_free(n);}
    number(heif_image_get_color_profile_type(image));size_t raw=heif_image_get_raw_color_profile_size(image);number(raw);
    if(raw){void* data=malloc(raw);error(heif_image_get_raw_color_profile(image,data));fwrite(data,1,raw,output);free(data);}
    int channels[]={0,1,2,3,4,5,6,10};
    for(unsigned c=0;c<sizeof(channels)/sizeof(channels[0]);c++){
      int channel=channels[c];int present=heif_image_has_channel(image,channel);number(present);if(!present)continue;
      uint32_t w=heif_image_get_width(image,channel),h=heif_image_get_height(image,channel),bits=heif_image_get_bits_per_pixel(image,channel);
      size_t stride;const uint8_t* plane=heif_image_get_plane_readonly2(image,channel,&stride);
      number(w);number(h);number(bits);number(heif_image_get_bits_per_pixel_range(image,channel));number(stride);
      for(uint32_t y=0;y<h;y++)fwrite(plane+y*stride,1,w*(bits/8),output);
    }
    heif_image_release(image);
  }
  heif_nclx_color_profile_free(options->output_image_nclx_profile);
  heif_decoding_options_free(options);
}
int main(int argc,char** argv){
  if(argc!=4)return 1;int mode=atoi(argv[3]);
  FILE* input=fopen(argv[1],"rb");if(!input)return 2;
  if(fseek(input,0,SEEK_END))return 3;long size=ftell(input);rewind(input);if(size<0)return 4;
  void* bytes=malloc((size_t)size+1);if(fread(bytes,1,size,input)!=(size_t)size)return 5;fclose(input);
  output=fopen(argv[2],"wb");if(!output)return 6;
  heif_context* ctx=heif_context_alloc();heif_error e=heif_context_read_from_memory(ctx,bytes,size,NULL);free(bytes);error(e);
  if(!e.code){uint32_t ids[100];int count=heif_context_get_list_of_top_level_image_IDs(ctx,ids,100);number(count);
    for(int i=0;i<count;i++){number(ids[i]);heif_image_handle* h=NULL;e=heif_context_get_image_handle(ctx,ids[i],&h);error(e);if(!e.code){decode(h,mode);heif_image_handle_release(h);}}
  }
  heif_context_free(ctx);return fclose(output)?7:0;
}
