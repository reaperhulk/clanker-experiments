/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Independent original-header client. Never compare padding or indeterminate
 * decoded coordinates returned by the upstream NCLX allocator. */
#include <libheif/heif.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e) { printf(" %d:%d:%s", e.code, e.subcode, e.message ? e.message : "NULL"); }
static void f32(float f) { uint32_t x; memcpy(&x,&f,4); printf(" %08" PRIx32,x); }
static void f64(double d) { uint64_t x; memcpy(&x,&d,8); printf(" %016" PRIx64,x); }
static void profile(heif_color_profile_nclx* p, int decoded) {
  printf(" %u:%d:%d:%d:%u",p->version,p->color_primaries,p->transfer_characteristics,p->matrix_coefficients,p->full_range_flag);
  if (decoded) {
    f32(p->color_primary_red_x); f32(p->color_primary_red_y);
    f32(p->color_primary_green_x); f32(p->color_primary_green_y);
    f32(p->color_primary_blue_x); f32(p->color_primary_blue_y);
    f32(p->color_primary_white_x); f32(p->color_primary_white_y);
  }
}
static void get_nclx(heif_image* image) {
  heif_color_profile_nclx* p=(void*)(uintptr_t)0x1234;
  heif_error e=heif_image_get_nclx_color_profile(image,&p);
  error(e); printf(" out=%d:%d",p==NULL,p==(void*)(uintptr_t)0x1234);
  if (!e.code) { profile(p,1); heif_nclx_color_profile_free(p); }
}
static void options(heif_color_conversion_options_ext* p) {
  printf(" %u:%d:%u:%u:%u:%u:%u:%u:%u",p->version,p->alpha_composition_mode,p->background_red,p->background_green,p->background_blue,p->secondary_background_red,p->secondary_background_green,p->secondary_background_blue,p->checkerboard_square_size);
}
static void mastering(heif_mastering_display_colour_volume* p) {
  for(int j=0;j<3;j++) printf(" %u:%u",p->display_primaries_x[j],p->display_primaries_y[j]);
  printf(" %u:%u:%" PRIu32 ":%" PRIu32,p->white_point_x,p->white_point_y,p->max_display_mastering_luminance,p->min_display_mastering_luminance);
}
int main(void) {
  heif_color_conversion_options opts; memset(&opts,0xa5,sizeof(opts));
  heif_color_conversion_options_set_defaults(&opts);
  printf("defaults %u %d %d %u",opts.version,opts.preferred_chroma_downsampling_algorithm,opts.preferred_chroma_upsampling_algorithm,opts.only_use_preferred_chroma_algorithm);
  heif_color_conversion_options_ext* ext=heif_color_conversion_options_ext_alloc(); if(!ext) return 1;
  options(ext); heif_color_conversion_options_ext_free(ext); heif_color_conversion_options_ext_free(NULL);
  heif_color_profile_nclx* allocated=heif_nclx_color_profile_alloc(); if(!allocated) return 2;
  profile(allocated,0); heif_nclx_color_profile_free(allocated); heif_nclx_color_profile_free(NULL); puts("");
  for(unsigned dv=0;dv<256;dv++) for(unsigned sv=0;sv<256;sv++) {
    heif_color_conversion_options_ext dst={0},src={0}; dst.version=dv; src.version=sv;
    src.alpha_composition_mode=2; src.background_red=17; src.background_green=123; src.background_blue=65535;
    src.secondary_background_red=65534; src.secondary_background_green=65533; src.secondary_background_blue=65532; src.checkerboard_square_size=65432;
    heif_color_conversion_options_ext_copy(&dst,NULL);
    heif_color_conversion_options_ext_copy(&dst,&src);
    heif_color_conversion_options_ext_copy(&dst,&dst);
    printf("options %u:%u",dv,sv); options(&dst); puts("");
  }
  for(unsigned value=0;value<65536;value++) {
    heif_color_profile_nclx p={0}; p.version=9; p.full_range_flag=7; p.color_primary_red_x=0.123f;
    printf("setters %u",value);
    error(heif_nclx_color_profile_set_color_primaries(&p,value));
    error(heif_nclx_color_profile_set_transfer_characteristics(&p,value));
    error(heif_nclx_color_profile_set_matrix_coefficients(&p,value)); profile(&p,1); puts("");
  }
  int vals[]={-1,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,22,65535,65536,65537,65538,0x7fffffff};
  for(unsigned a=0;a<sizeof(vals)/sizeof(vals[0]);a++) for(unsigned b=0;b<sizeof(vals)/sizeof(vals[0]);b++) for(unsigned r=0;r<3;r++) {
    heif_image* image=NULL; if(heif_image_create(1,1,heif_colorspace_RGB,heif_chroma_444,&image).code) return 3;
    printf("nclx %d:%d:%u type=%u",vals[a],vals[b],r,(unsigned)heif_image_get_color_profile_type(image));
    get_nclx(image); error(heif_image_get_nclx_color_profile(image,NULL));
    heif_color_profile_nclx p={0}; p.version=0; p.color_primaries=vals[a]; p.transfer_characteristics=vals[b]; p.matrix_coefficients=(a%2)?vals[b]:2; p.full_range_flag=r?((r==1)?1:255):0;
    error(heif_image_set_nclx_color_profile(image,&p));
    p.color_primaries=99; p.color_primary_red_x=123; /* input is copied */
    printf(" type=%u",(unsigned)heif_image_get_color_profile_type(image)); get_nclx(image);
    heif_image_release(image); puts("");
  }
  const char* types[]={"","a","abc","prof","rICC","nclx","zzzz","profx","p\0of","\xff\xff\xff\xff"};
  unsigned sizes[]={0,1,3,4,255,256,1024};
  for(unsigned t=0;t<sizeof(types)/sizeof(types[0]);t++) for(unsigned s=0;s<sizeof(sizes)/sizeof(sizes[0]);s++) {
    heif_image* image=NULL; if(heif_image_create(1,1,heif_colorspace_RGB,heif_chroma_444,&image).code) return 4;
    printf("raw %u:%u",t,sizes[s]);
    unsigned char data[1024],out[1026]; memset(out,0xa5,sizeof(out));
    error(heif_image_get_raw_color_profile(image,out+1)); error(heif_image_get_raw_color_profile(image,NULL));
    heif_color_profile_nclx p={0}; p.color_primaries=1; p.transfer_characteristics=13; p.matrix_coefficients=6;
    error(heif_image_set_nclx_color_profile(image,&p));
    for(int round=0;round<2;round++) {
      for(unsigned i=0;i<sizeof(data);i++) data[i]=(i*37+round*11)&255;
      error(heif_image_set_raw_color_profile(image,types[t],data,sizes[s])); memset(data,0xa5,sizeof(data));
      printf(" type=%u size=%zu",(unsigned)heif_image_get_color_profile_type(image),heif_image_get_raw_color_profile_size(image));
      error(heif_image_get_raw_color_profile(image,out+1));
      for(unsigned i=0;i<sizes[s]+2;i++) printf("%02x",out[i]);
    }
    get_nclx(image); heif_image_release(image); puts("");
  }
  for(unsigned value=0;value<65536;value++) {
    heif_mastering_display_colour_volume in={0}; heif_decoded_mastering_display_colour_volume out;
    for(unsigned j=0;j<3;j++) { in.display_primaries_x[j]=(value+j)&65535; in.display_primaries_y[j]=(value+65535-j)&65535; }
    in.white_point_x=value; in.white_point_y=value;
    in.max_display_mastering_luminance=value*1703u; in.min_display_mastering_luminance=value;
    printf("decode %u",value); error(heif_mastering_display_colour_volume_decode(&in,&out));
    for(unsigned j=0;j<3;j++) { f32(out.display_primaries_x[j]); f32(out.display_primaries_y[j]); }
    f32(out.white_point_x); f32(out.white_point_y); f64(out.max_display_mastering_luminance); f64(out.min_display_mastering_luminance); puts("");
  }
  uint32_t bounds[]={0,1,4,5,37000,37001,42000,42001,49999,50000,50001,99999999,100000000,100000001,UINT32_MAX};
  for(unsigned a=0;a<sizeof(bounds)/sizeof(bounds[0]);a++) for(unsigned b=0;b<sizeof(bounds)/sizeof(bounds[0]);b++) {
    heif_image* image=NULL; if(heif_image_create(1,1,heif_colorspace_RGB,heif_chroma_444,&image).code) return 5;
    heif_content_light_level light={bounds[a],bounds[b]},lg={123,456};
    heif_ambient_viewing_environment ambient={bounds[a],bounds[b],bounds[a]},ag={123,456,789};
    printf("hdr %u:%u initial=%d:%d:%d:%d:%" PRIu32,a,b,heif_image_has_content_light_level(image),heif_image_has_mastering_display_colour_volume(image),heif_image_has_ambient_viewing_environment(image),heif_image_has_nominal_diffuse_white_luminance(image),heif_image_get_nominal_diffuse_white_luminance(image));
    heif_image_get_content_light_level(image,&lg); printf(" light=%u:%u",lg.max_content_light_level,lg.max_pic_average_light_level);
    int present=heif_image_get_ambient_viewing_environment(image,&ag); printf(" ambient=%d:%" PRIu32 ":%u:%u",present,ag.ambient_illumination,ag.ambient_light_x,ag.ambient_light_y);
    heif_image_set_content_light_level(image,NULL); heif_image_set_mastering_display_colour_volume(image,NULL); heif_image_set_ambient_viewing_environment(image,NULL);
    heif_image_get_content_light_level(image,NULL); printf(" absent=%d",heif_image_get_ambient_viewing_environment(image,NULL));
    heif_image_set_content_light_level(image,&light); heif_image_set_ambient_viewing_environment(image,&ambient); memset(&light,0,sizeof(light)); memset(&ambient,0,sizeof(ambient));
    heif_image_set_nominal_diffuse_white_luminance(image,bounds[a]);
    heif_image_get_content_light_level(image,&lg); present=heif_image_get_ambient_viewing_environment(image,&ag);
    printf(" stored=%d:%d:%d:%" PRIu32 " light=%u:%u ambient=%d:%" PRIu32 ":%u:%u",heif_image_has_content_light_level(image),heif_image_has_ambient_viewing_environment(image),heif_image_has_nominal_diffuse_white_luminance(image),heif_image_get_nominal_diffuse_white_luminance(image),lg.max_content_light_level,lg.max_pic_average_light_level,present,ag.ambient_illumination,ag.ambient_light_x,ag.ambient_light_y);
    heif_mastering_display_colour_volume in={0},got={0}; heif_decoded_mastering_display_colour_volume decoded={0};
    in.display_primaries_x[0]=bounds[a]; in.display_primaries_y[2]=bounds[b]; in.white_point_x=bounds[b]; in.white_point_y=bounds[a]; in.max_display_mastering_luminance=bounds[a]; in.min_display_mastering_luminance=bounds[b];
    heif_image_set_mastering_display_colour_volume(image,&in); memset(&in,0,sizeof(in)); heif_image_get_mastering_display_colour_volume(image,&got);
    printf(" mastering=%d",heif_image_has_mastering_display_colour_volume(image)); mastering(&got);
    error(heif_mastering_display_colour_volume_decode(&got,&decoded)); f64(decoded.max_display_mastering_luminance); f64(decoded.min_display_mastering_luminance);
    error(heif_mastering_display_colour_volume_decode(NULL,&decoded)); error(heif_mastering_display_colour_volume_decode(&got,NULL));
    heif_image_set_content_light_level(image,&light); printf(" reset=%d",heif_image_has_content_light_level(image));
    heif_image_release(image); puts("");
  }
  return 0;
}
