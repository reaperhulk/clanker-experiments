/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){printf(" e=%d,%d,%s",e.code,e.subcode,e.message?e.message:"NULL");}
static heif_image* create(int w,int h,int cs,int ch,int depth,int alpha,int broken){
  heif_image* i=NULL;heif_error e=heif_image_create(w,h,cs,ch,&i);if(e.code)abort();
  int channels[4],count=0;
  if(ch>=10)channels[count++]=10;
  else if(cs==1){channels[count++]=3;channels[count++]=4;channels[count++]=5;}
  else{channels[count++]=0;if(cs==0){channels[count++]=1;channels[count++]=2;}}
  if(alpha && ch<10)channels[count++]=6;
  for(int c=0;c<count;c++){
    int channel=channels[c];int sub=channel==1||channel==2;
    int pw=sub&&(ch==1||ch==2)?(w+1)/2:w,ph=sub&&ch==1?(h+1)/2:h;
    if(broken && c==0)pw++;
    e=heif_image_add_plane(i,channel,pw,ph,depth);if(e.code)abort();
    size_t stride;uint8_t* p=heif_image_get_plane2(i,channel,&stride);
    int components=ch>=10?(ch%2?4:3):1;
    for(int y=0;y<ph;y++)for(int x=0;x<pw*components;x++){
      unsigned v=(x*173+y*281+channel*61+((x*y)%19))&((1u<<depth)-1);
      if(depth<=8)p[y*stride+x]=v;
      else{uint16_t vv=v;memcpy(p+y*stride+2*x,&vv,2);}
    }
  }
  heif_image_set_premultiplied_alpha(i,1);heif_image_set_pixel_aspect_ratio(i,3,2);
  heif_color_profile_nclx* n=heif_nclx_color_profile_alloc();n->color_primaries=1;n->matrix_coefficients=6;n->transfer_characteristics=13;n->full_range_flag=1;
  e=heif_image_set_nclx_color_profile(i,n);if(e.code)abort();heif_nclx_color_profile_free(n);
  const unsigned char icc[]={17,0,19,41};e=heif_image_set_raw_color_profile(i,"prof",icc,sizeof(icc));if(e.code)abort();
  return i;
}
static void dump(heif_image* i){
  uint32_t a,b;heif_image_get_pixel_aspect_ratio(i,&a,&b);
  printf(" im=%d,%d,%d,%d,%d,%u,%u",heif_image_get_primary_width(i),heif_image_get_primary_height(i),heif_image_get_colorspace(i),heif_image_get_chroma_format(i),heif_image_is_premultiplied_alpha(i),a,b);
  heif_color_profile_nclx* n=NULL;error(heif_image_get_nclx_color_profile(i,&n));if(n){printf(" n=%d,%d,%d,%d",n->color_primaries,n->transfer_characteristics,n->matrix_coefficients,n->full_range_flag);heif_nclx_color_profile_free(n);}
  size_t raw=heif_image_get_raw_color_profile_size(i);printf(" icc=%u,%zu",heif_image_get_color_profile_type(i),raw);if(raw){uint8_t p[16];if(raw>sizeof(p))abort();error(heif_image_get_raw_color_profile(i,p));for(size_t x=0;x<raw;x++)printf("%02x",p[x]);}
  int channels[]={0,1,2,3,4,5,6,10};
  for(unsigned c=0;c<8;c++){int channel=channels[c];if(!heif_image_has_channel(i,channel))continue;
    int w=heif_image_get_width(i,channel),h=heif_image_get_height(i,channel),bits=heif_image_get_bits_per_pixel(i,channel);size_t stride;const uint8_t* p=heif_image_get_plane_readonly2(i,channel,&stride);
    printf(" p=%d,%d,%d,%d,%d,%zu:",channel,w,h,bits,heif_image_get_bits_per_pixel_range(i,channel),stride);
    for(int y=0;y<h;y++)for(int x=0;x<w*(bits/8);x++)printf("%02x",p[y*stride+x]);
  }
}
int main(void){
  const int configs[][2]={{2,0},{0,1},{0,2},{0,3},{1,3},{1,10},{1,11},{1,12},{1,13},{1,14},{1,15}};
  const int depths[]={8,10,12,16};const int sizes[][2]={{1,1},{2,3},{3,2},{5,7},{8,6},{9,9}};
  const int margins[][4]={{0,0,0,0},{1,0,0,0},{0,1,0,0},{0,0,1,0},{0,0,0,1},{1,1,1,1},{2,1,2,1},{-1,0,0,0},{20,0,0,0}};
  for(unsigned config=0;config<11;config++)for(unsigned d=0;d<4;d++){
    int cs=configs[config][0],ch=configs[config][1],depth=depths[d];if((ch==10||ch==11)&&depth!=8)continue;if(ch>=12&&depth<=8)continue;
    for(unsigned size=0;size<6;size++)for(int alpha=0;alpha<2;alpha++)for(int broken=0;broken<2;broken++){
      int w=sizes[size][0],h=sizes[size][1];
      for(unsigned m=0;m<9;m++){
        heif_image* i=create(w,h,cs,ch,depth,alpha,broken);printf("crop %u %u %u %d %d %u",config,d,size,alpha,broken,m);
        error(heif_image_crop(i,margins[m][0],margins[m][1],margins[m][2],margins[m][3]));dump(i);puts("");heif_image_release(i);
      }
      for(unsigned target=0;target<6;target++){
        heif_image* i=create(w,h,cs,ch,depth,alpha,broken);heif_image* out=(void*)(uintptr_t)0x1234;
        printf("scale %u %u %u %d %d %u",config,d,size,alpha,broken,target);heif_error e=heif_image_scale_image(i,&out,sizes[target][0],sizes[target][1],NULL);error(e);
        printf(" out=%d,%d",out==NULL,out==(void*)(uintptr_t)0x1234);if(!e.code){dump(out);heif_image_release(out);}dump(i);puts("");heif_image_release(i);
      }
    }
  }
  return ferror(stdout)?1:0;
}
