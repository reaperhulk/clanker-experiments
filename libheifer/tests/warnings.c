/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "warning_values.h"
static void fail(void){abort();}
static heif_image* image(void){heif_image* i=NULL;if(heif_image_create(4,4,heif_colorspace_YCbCr,heif_chroma_420,&i).code)fail();if(heif_image_add_plane(i,heif_channel_Y,4,4,8).code)fail();if(heif_image_add_plane(i,heif_channel_Cb,2,2,8).code)fail();if(heif_image_add_plane(i,heif_channel_Cr,2,2,8).code)fail();return i;}
int main(void){
  for(unsigned c=0;c<sizeof(codes)/sizeof(*codes);c++)for(unsigned s=0;s<sizeof(subcodes)/sizeof(*subcodes);s++){
    heif_image* i=image();heif_error in={(heif_error_code)codes[c],(heif_suberror_code)subcodes[s],"caller text must be ignored"};
    heif_image_add_decoding_warning(i,in);heif_error out={0};
    printf("value %d %d %d %d ",codes[c],subcodes[s],heif_image_get_decoding_warnings(i,INT_MIN,NULL,0),heif_image_get_decoding_warnings(i,0,&out,1));
    printf("%d %d %s\n",out.code,out.subcode,out.message);heif_image_release(i);
  }
  heif_image* i=image();
  for(int n=0;n<7;n++){heif_error e={heif_error_Invalid_input,heif_suberror_No_ftyp_box,NULL};heif_image_add_decoding_warning(i,e);}
  for(int first=0;first<10;first++)for(int cap=-2;cap<11;cap++){
    heif_error out[12];for(unsigned n=0;n<12;n++){out[n].code=(heif_error_code)99;out[n].subcode=(heif_suberror_code)99;out[n].message="sentinel";}
    int count=heif_image_get_decoding_warnings(i,first,out,cap);
    printf("page %d %d %d",first,cap,count);
    for(unsigned n=0;n<12;n++)printf(" | %d,%d,%s",out[n].code,out[n].subcode,out[n].message);puts("");
  }
  heif_error crop=heif_image_crop(i,1,0,0,1);if(crop.code)fail();printf("crop %d ",crop.code);printf("%d\n",heif_image_get_decoding_warnings(i,0,NULL,0));
  heif_image* scaled=NULL;heif_error scale=heif_image_scale_image(i,&scaled,2,2,NULL);if(scale.code)fail();printf("scale %d ",scale.code);printf("%d\n",heif_image_get_decoding_warnings(scaled,0,NULL,0));
  heif_image_release(scaled);heif_image_release(i);
  printf("threads null %d\n",heif_context_get_max_decoding_threads(NULL));
  heif_context* ctx=heif_context_alloc();printf("threads default %d\n",heif_context_get_max_decoding_threads(ctx));
  const int values[]={INT_MIN,-100,-1,0,1,2,4,64,INT_MAX};
  for(unsigned n=0;n<sizeof(values)/sizeof(*values);n++){heif_context_set_max_decoding_threads(ctx,values[n]);printf("threads set %d %d\n",values[n],heif_context_get_max_decoding_threads(ctx));}
  heif_context_free(ctx);return 0;
}
