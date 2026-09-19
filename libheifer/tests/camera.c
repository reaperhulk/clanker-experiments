/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_properties.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){printf(" e%d:%d:",e.code,e.subcode);for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}
static void values(const double* v,unsigned n){for(unsigned i=0;i<n;i++){uint64_t bits;memcpy(&bits,v+i,8);printf(":%016llx",(unsigned long long)bits);}}
static void rotation(const heif_camera_extrinsic_matrix* m){double v[9];memset(v,0x5a,sizeof(v));error(heif_camera_extrinsic_matrix_get_rotation_matrix(m,v));values(v,9);error(heif_camera_extrinsic_matrix_get_rotation_matrix(m,NULL));}
static void snapshot(heif_image_handle* h){
  printf(" has%d:%d",heif_image_handle_has_camera_intrinsic_matrix(h),heif_image_handle_has_camera_extrinsic_matrix(h));
  heif_camera_intrinsic_matrix m;memset(&m,0x5a,sizeof(m));error(heif_image_handle_get_camera_intrinsic_matrix(h,&m));double v[]={m.focal_length_x,m.focal_length_y,m.principal_point_x,m.principal_point_y,m.skew};values(v,5);
  heif_camera_extrinsic_matrix* a=(void*)(uintptr_t)0x1234;heif_camera_extrinsic_matrix* b=(void*)(uintptr_t)0x1234;
  heif_error e=heif_image_handle_get_camera_extrinsic_matrix(h,&a);error(e);printf(" out%d:%d",a==NULL,a==(void*)(uintptr_t)0x1234);
  if(!e.code){rotation(a);e=heif_image_handle_get_camera_extrinsic_matrix(h,&b);error(e);printf(" separate%d",a!=b);if(!e.code){rotation(b);heif_camera_extrinsic_matrix_release(b);}heif_camera_extrinsic_matrix_release(a);}
  error(heif_image_handle_get_camera_intrinsic_matrix(h,NULL));error(heif_image_handle_get_camera_extrinsic_matrix(h,NULL));
}
int main(void){
  uint32_t n;unsigned i=0;while(fread(&n,4,1,stdin)==1){if(n>2000000)return 1;void* data=malloc(n);if(fread(data,1,n,stdin)!=n)return 2;
    heif_context* c=heif_context_alloc();printf("case%u",i++);error(heif_context_read_from_memory(c,data,n,NULL));free(data);
    for(unsigned p=0;p<10;p++)printf(" property%08x",heif_item_get_property_type(c,1,p));
    heif_image_handle* h=NULL;heif_error e=heif_context_get_image_handle(c,1,&h);error(e);
    heif_camera_extrinsic_matrix* owned=NULL;
    if(!e.code){snapshot(h);error(heif_image_handle_get_camera_extrinsic_matrix(h,&owned));
      int w=heif_image_handle_get_width(h),height=heif_image_handle_get_height(h);printf(" dim%d:%d",w,height);
      if(w>0&&height>0&&w<=256&&height<=256){heif_image* image=NULL;e=heif_decode_image(h,&image,heif_colorspace_undefined,heif_chroma_undefined,NULL);error(e);
        if(!e.code){int n=heif_image_get_decoding_warnings(image,0,NULL,0);printf(" warnings%d",n);for(int j=0;j<n;j++){heif_error warning;printf(":%d",heif_image_get_decoding_warnings(image,j,&warning,1));error(warning);}heif_image_release(image);}}
    }
    error(heif_context_read_from_memory(c,NULL,0,NULL));if(h)snapshot(h);heif_context_free(c);if(h){snapshot(h);heif_image_handle_release(h);}
    if(owned){rotation(owned);heif_camera_extrinsic_matrix_release(owned);}puts("");
  }
  snapshot(NULL);rotation(NULL);heif_camera_extrinsic_matrix_release(NULL);puts("");return 0;
}
