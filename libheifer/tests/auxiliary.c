/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void error(heif_error e){printf(" e%d:%d:",e.code,e.subcode);for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}
static void string(const char* s){printf(" s");for(const unsigned char* p=(const unsigned char*)s;p&&*p;p++)printf("%02x",*p);}
static void representation(heif_image_handle* h){
  for(unsigned id=0;id<3;id++){
    const heif_depth_representation_info* d=(void*)(uintptr_t)0x1234;
    int n=heif_image_handle_get_depth_image_representation_info(h,id==2?999:id,&d);
    printf(" r%d:%d:%d",n,d==NULL,d==(void*)(uintptr_t)0x1234);
    if(n){printf(":%u:%u:%u:%u:%u:%d:%u:%u:%d",d->version,d->has_z_near,d->has_z_far,d->has_d_min,d->has_d_max,d->depth_representation_type,d->disparity_reference_view,d->depth_nonlinear_representation_model_size,d->depth_nonlinear_representation_model==NULL);
      const double values[]={d->has_z_near?d->z_near:0,d->has_z_far?d->z_far:0,d->has_d_min?d->d_min:0,d->has_d_max?d->d_max:0};
      for(unsigned j=0;j<4;j++){uint64_t bits;memcpy(&bits,&values[j],8);printf(":%016llx",(unsigned long long)bits);}heif_depth_representation_info_free(d);
    }
  }
  printf(" rn%d",heif_image_handle_get_depth_image_representation_info(h,0,NULL));
}
static void describe(heif_image_handle* h){
  printf(" id%u:%d:%d",heif_image_handle_get_item_id(h),heif_image_handle_get_width(h),heif_image_handle_get_height(h));
  const char *a=NULL,*b=NULL;
  error(heif_image_handle_get_auxiliary_type(h,&a));error(heif_image_handle_get_auxiliary_type(h,&b));string(a);string(b);printf(" separate%d",a!=b);
  heif_image_handle_release_auxiliary_type(h,&a);heif_image_handle_free_auxiliary_types(NULL,&b);printf(" freed%d:%d",a==NULL,b==NULL);heif_image_handle_release_auxiliary_type(NULL,&a);heif_image_handle_release_auxiliary_type(NULL,NULL);
  error(heif_image_handle_get_auxiliary_type(h,NULL));representation(h);
}
static void snapshot(heif_image_handle* h){
  describe(h);
  int filters[]={-1,0,1,2,3,4,5,6,7,8,16};int counts[]={-3,-1,0,1,2,4};
  for(unsigned f=0;f<sizeof(filters)/sizeof(filters[0]);f++){
    printf(" f%d:%d",filters[f],heif_image_handle_get_number_of_auxiliary_images(h,filters[f]));
    for(unsigned c=0;c<sizeof(counts)/sizeof(counts[0]);c++){
      heif_item_id ids[4]={999,999,999,999};int n=heif_image_handle_get_list_of_auxiliary_image_IDs(h,filters[f],ids,counts[c]);printf(" l%d",n);for(unsigned i=0;i<4;i++)printf(":%u",ids[i]);
    }
    printf(" null%d",heif_image_handle_get_list_of_auxiliary_image_IDs(h,filters[f],NULL,4));
  }
  printf(" depth%d:%d",heif_image_handle_has_depth_image(h),heif_image_handle_get_number_of_depth_images(h));
  for(unsigned c=0;c<sizeof(counts)/sizeof(counts[0]);c++){heif_item_id ids[2]={999,999};int n=heif_image_handle_get_list_of_depth_image_IDs(h,ids,counts[c]);printf(" d%d:%u:%u",n,ids[0],ids[1]);}
  for(unsigned id=0;id<9;id++){
    heif_image_handle* child=(void*)(uintptr_t)0x1234;heif_error e=heif_image_handle_get_auxiliary_image_handle(h,id,&child);error(e);printf(" a%d:%d",child==NULL,child==(void*)(uintptr_t)0x1234);if(!e.code){describe(child);heif_image_handle_release(child);}
    child=(void*)(uintptr_t)0x1234;e=heif_image_handle_get_depth_image_handle(h,id,&child);error(e);printf(" d%d:%d",child==NULL,child==(void*)(uintptr_t)0x1234);if(!e.code){describe(child);heif_image_handle_release(child);}
  }
  error(heif_image_handle_get_depth_image_handle(h,0,NULL));error(heif_image_handle_get_auxiliary_image_handle(h,0,NULL));
}
int main(void){
  uint32_t n,m;unsigned case_id=0;
  while(fread(&n,4,1,stdin)==1){if(fread(&m,4,1,stdin)!=1||n>2000000||m>2000000)return 1;void* a=malloc(n);void* b=malloc(m);if(fread(a,1,n,stdin)!=n||fread(b,1,m,stdin)!=m)return 2;
    heif_context* c=heif_context_alloc();printf("case%u",case_id++);error(heif_context_read_from_memory(c,a,n,NULL));
    heif_image_handle* handles[8]={0};
    for(unsigned i=0;i<8;i++){error(heif_context_get_image_handle(c,i+1,&handles[i]));if(handles[i])snapshot(handles[i]);}
    error(heif_context_read_from_memory(c,b,m,NULL));
    for(unsigned i=0;i<8;i++)if(handles[i])snapshot(handles[i]);
    heif_context_free(c);
    for(unsigned i=0;i<8;i++)if(handles[i]){snapshot(handles[i]);heif_image_handle_release(handles[i]);}
    free(a);free(b);puts("");
  }
  heif_depth_representation_info_free(NULL);return 0;
}
