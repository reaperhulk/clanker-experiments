/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_properties.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){printf(" e%d:%d:",e.code,e.subcode);for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}
static void floats(const float* v,unsigned n){for(unsigned i=0;i<n;i++){uint32_t bits;memcpy(&bits,v+i,4);printf(":%08x",bits);}}
static void ints(const uint32_t* v,unsigned n){for(unsigned i=0;i<n;i++)printf(":%u",v[i]);}
static void reference(heif_image* image){uint32_t id=999;error(heif_image_add_bayer_component(image,65535,&id));printf(" ref%u",id);}
static void snapshot(heif_image* image){
  printf(" counts%d:%d:%d chroma%d:%u",heif_image_get_number_of_polarization_patterns(image),heif_image_get_number_of_sensor_bad_pixels_maps(image),heif_image_get_number_of_sensor_nucs(image),heif_image_has_chroma_location(image),heif_image_get_chroma_location(image));
  uint32_t component_ids[]={0,1,2,3,4,7,999,UINT32_MAX};
  for(unsigned k=0;k<8;k++){
    uint16_t w=999,h=999;printf(" bayer%d",heif_image_get_bayer_pattern_size(image,component_ids[k],&w,&h));printf(":%u:%u",w,h);printf(":%d",heif_image_get_bayer_pattern_size(image,component_ids[k],NULL,NULL));
    heif_bayer_pattern_pixel p[16];memset(p,0xa5,sizeof(p));heif_error e=heif_image_get_bayer_pattern(image,component_ids[k],p);error(e);for(unsigned j=0;j<(!e.code?(unsigned)w*h:1);j++){printf(":%u",p[j].component_id);floats(&p[j].component_gain,1);}error(heif_image_get_bayer_pattern(image,component_ids[k],NULL));
    printf(" polar-index%d",heif_image_get_polarization_pattern_index_for_component(image,component_ids[k]));
  }
  int indices[]={-2147483647-1,-1,0,1,2,3,2147483647};
  for(unsigned k=0;k<7;k++){
    int index=indices[k];uint32_t count=999,w=999,h=999,rows=999,cols=999,pixels=999;uint16_t pw=999,ph=999;int applied=999;
    uint32_t ids[16],r[16],c[16];float a[16],b[16];heif_bad_pixel p[16];memset(ids,0xa5,sizeof(ids));memset(r,0xa5,sizeof(r));memset(c,0xa5,sizeof(c));memset(a,0xa5,sizeof(a));memset(b,0xa5,sizeof(b));memset(p,0xa5,sizeof(p));
    heif_error e=heif_image_get_polarization_pattern_info(image,index,&count,&pw,&ph);error(e);printf(" info%u:%u:%u",count,pw,ph);error(heif_image_get_polarization_pattern_info(image,index,NULL,NULL,NULL));
    error(heif_image_get_polarization_pattern_data(image,index,ids,a));ints(ids,!e.code?count:1);floats(a,!e.code?(unsigned)pw*ph:1);error(heif_image_get_polarization_pattern_data(image,index,NULL,a));error(heif_image_get_polarization_pattern_data(image,index,ids,NULL));
    e=heif_image_get_sensor_bad_pixels_map_info(image,index,&count,&applied,&rows,&cols,&pixels);error(e);printf(" info%u:%d:%u:%u:%u",count,applied,rows,cols,pixels);error(heif_image_get_sensor_bad_pixels_map_info(image,index,NULL,NULL,NULL,NULL,NULL));
    error(heif_image_get_sensor_bad_pixels_map_data(image,index,ids,r,c,p));ints(ids,!e.code?count:1);ints(r,!e.code?rows:1);ints(c,!e.code?cols:1);for(unsigned j=0;j<(!e.code?pixels:1);j++)printf(":%u:%u",p[j].row,p[j].column);error(heif_image_get_sensor_bad_pixels_map_data(image,index,NULL,NULL,NULL,NULL));
    e=heif_image_get_sensor_nuc_info(image,index,&count,&applied,&w,&h);error(e);printf(" info%u:%d:%u:%u",count,applied,w,h);error(heif_image_get_sensor_nuc_info(image,index,NULL,NULL,NULL,NULL));
    error(heif_image_get_sensor_nuc_data(image,index,ids,a,b));ints(ids,!e.code?count:1);floats(a,!e.code?w*h:1);floats(b,!e.code?w*h:1);error(heif_image_get_sensor_nuc_data(image,index,NULL,NULL,NULL));
  }
}
static void add(heif_image* image,const uint32_t* v){
  uint32_t flags=v[1],width=v[2],height=v[3],count=v[4];int applied=(int32_t)v[5];
  uint32_t ids[16],rows[16],cols[16];float a[16],b[16];heif_bad_pixel bad[16];heif_bayer_pattern_pixel bayer[16];
  for(unsigned j=0;j<16;j++){ids[j]=j%3==0?UINT32_MAX:j%3;rows[j]=j+100;cols[j]=UINT32_MAX-j;bad[j]=(heif_bad_pixel){rows[j],cols[j]};uint32_t bits=v[6]^(j<<4);memcpy(a+j,&bits,4);bits^=0x80000000;memcpy(b+j,&bits,4);bayer[j]=(heif_bayer_pattern_pixel){ids[j],a[j]};}
  error(heif_image_set_bayer_pattern(image,999,width,height,(flags&1)?NULL:bayer));
  error(heif_image_add_polarization_pattern(image,count,(flags&2)?NULL:ids,width,height,(flags&4)?NULL:a));
  error(heif_image_add_sensor_bad_pixels_map(image,count,(flags&2)?NULL:ids,applied,count,(flags&8)?NULL:rows,count,(flags&16)?NULL:cols,count,(flags&32)?NULL:bad));
  error(heif_image_add_sensor_nuc(image,count,(flags&2)?NULL:ids,applied,width,height,(flags&4)?NULL:a,(flags&8)?NULL:b));
  for(unsigned j=0;j<16;j++){ids[j]=7;rows[j]=9;cols[j]=11;bad[j]=(heif_bad_pixel){13,15};a[j]=0;b[j]=0;bayer[j]=(heif_bayer_pattern_pixel){17,0};}
  error(heif_image_add_polarization_pattern(image,0,NULL,1,1,a));error(heif_image_add_polarization_pattern(image,1,ids,1,1,b));
  error(heif_image_add_sensor_bad_pixels_map(image,0,NULL,-1,0,NULL,0,NULL,0,NULL));
  error(heif_image_add_sensor_nuc(image,0,NULL,-1,1,1,a,b));
}
int main(void){
  uint32_t v[9];unsigned i=0;while(fread(v,sizeof(v),1,stdin)==1){printf("case%u",i++);
    if(v[0]==0){float f;memcpy(&f,&v[6],4);printf(" no-filter%d",heif_polarization_angle_is_no_filter(f));f=heif_polarization_angle_no_filter();floats(&f,1);puts("");continue;}
    heif_image* image=NULL;error(heif_image_create(8,8,(heif_colorspace)v[8],(heif_chroma)v[7],&image));if(!image)return 1;
    if(v[0]==1){error(heif_image_set_chroma_location(image,2));error(heif_image_set_chroma_location(image,(uint8_t)v[6]));printf(" location%d:%u",heif_image_has_chroma_location(image),heif_image_get_chroma_location(image));heif_image_release(image);puts("");continue;}
    reference(image);int channels[4]={0,1,2,6};int n=1;if(v[8]==0)n=3;else if(v[8]==1)channels[0]=10;
    for(int k=0;k<n;k++){int subsampled=v[8]==0&&k>0;error(heif_image_add_plane(image,(heif_channel)channels[k],subsampled?4:8,subsampled?4:8,8));}
    if(v[1]&16)error(heif_image_add_plane(image,(heif_channel)channels[0],8,8,8));reference(image);snapshot(image);add(image,v);error(heif_image_set_chroma_location(image,6));snapshot(image);
    heif_image* scaled=NULL;heif_error e=heif_image_scale_image(image,&scaled,4,4,NULL);error(e);if(!e.code){snapshot(scaled);reference(scaled);}
    error(heif_image_crop(image,1,2,1,2));snapshot(image);reference(image);heif_image_release(image);if(scaled){snapshot(scaled);heif_image_release(scaled);}puts("");
  }
  snapshot(NULL);uint32_t id=999;error(heif_image_add_bayer_component(NULL,0,&id));printf(" id%u",id);uint32_t v0[9]={2,0,2,3,3,0,0,0,2};add(NULL,v0);error(heif_image_set_chroma_location(NULL,255));puts("");return 0;
}
