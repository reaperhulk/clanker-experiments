/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main unused_transforms_main
#define error transforms_error
#include "transforms.c"
#undef main
#undef error
#include <libheif/heif_sequences.h>
#include <libheif/heif_properties.h>
static void error(heif_error e){printf(" e%d/%d/",e.code,e.subcode);if(e.message)for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}
static void string(const char* s){if(!s){printf("NULL");return;}printf("[");for(const unsigned char* p=(const unsigned char*)s;*p;p++)printf("%02x",*p);printf("]");}
static void snapshot(heif_image_handle* h){
 printf(" h=");const char* a=heif_image_handle_get_gimi_content_id(h),*b=heif_image_handle_get_gimi_content_id(h);string(a);printf("/%d/",a==b);if(a&&*a)((char*)a)[0]^=0x55;string(b);heif_string_release(a);heif_string_release(b);
 int n=heif_image_handle_has_gimi_component_content_ids(h);printf(" c%d",n);if(n<0||n>1024)abort();
 for(int j=0;j<n+3;j++){const char* a=heif_image_handle_get_gimi_component_content_id(h,j);const char* b=heif_image_handle_get_gimi_component_content_id(h,j);string(a);printf("/%d/",a==b);if(a&&*a)((char*)a)[0]^=0x66;string(b);heif_string_release(a);heif_string_release(b);}
 string(heif_image_handle_get_gimi_component_content_id(h,UINT32_MAX));
}
static void pixel_id(heif_image* image){const char* id=heif_image_get_gimi_sample_content_id(image);printf(" pixelid");string(id);heif_string_release(id);dump(image);}
static void decoded(heif_image_handle* h){
 heif_image* image=(void*)0x1234;heif_error e=heif_decode_image(h,&image,heif_colorspace_undefined,heif_chroma_undefined,NULL);error(e);printf(" out%d/%d",image==NULL,image==(void*)0x1234);
 if(!e.code){pixel_id(image);heif_image* scaled=NULL;error(heif_image_scale_image(image,&scaled,2,2,NULL));if(scaled){pixel_id(scaled);heif_image_release(scaled);}heif_image_release(image);}
}
static void props(heif_context* ctx){for(uint32_t id=1;id<=2;id++){uint32_t ids[64];int n=heif_item_get_properties_of_type(ctx,id,0,ids,64);printf(" props%u/%d",id,n);for(int j=0;j<n;j++)printf("/%u/%u",ids[j],heif_item_get_property_type(ctx,id,ids[j]));}}
int main(void){uint32_t v[6];unsigned number=0;while(fread(v,sizeof(v),1,stdin)==1){
 uint8_t* file=malloc(v[0]+1),*reload=malloc(v[1]+1);char* text=calloc(v[4]+1,1);if(fread(file,1,v[0],stdin)!=v[0]||fread(reload,1,v[1],stdin)!=v[1]||fread(text,1,v[4],stdin)!=v[4])return 2;
 heif_context* ctx=heif_context_alloc();if(v[3]&1)heif_context_get_security_limits(ctx)->max_components=v[5];printf("case%u",number++);error(heif_context_read_from_memory(ctx,file,v[0],NULL));free(file);props(ctx);
 heif_image_handle* h=NULL,*alias=NULL,*other=NULL;error(heif_context_get_primary_image_handle(ctx,&h));
 if(h){error(heif_context_get_primary_image_handle(ctx,&alias));error(heif_context_get_image_handle(ctx,2,&other));snapshot(h);decoded(h);if(other)snapshot(other);
  heif_image_handle_set_gimi_content_id(h,text);heif_image_handle_set_gimi_component_content_id(h,v[2],v[3]&2?NULL:text);memset(text,0xee,v[4]);snapshot(alias);if(other)snapshot(other);props(ctx);decoded(h);
  if(other){heif_image_handle_set_gimi_component_content_id(other,v[2]+1,"other");snapshot(h);snapshot(other);}
  heif_image_handle_set_gimi_content_id(alias,"");heif_image_handle_set_gimi_component_content_id(alias,0,"");snapshot(h);decoded(h);
 }
 error(heif_context_read_from_memory(ctx,reload,v[1],NULL));free(reload);free(text);props(ctx);heif_image_handle* newer=NULL;error(heif_context_get_primary_image_handle(ctx,&newer));if(newer){snapshot(newer);decoded(newer);}if(h){snapshot(h);decoded(h);heif_image_handle_set_gimi_content_id(h,"after reload");heif_image_handle_set_gimi_component_content_id(h,0,"after reload");snapshot(h);if(newer)snapshot(newer);}
 heif_context_free(ctx);if(h){snapshot(h);decoded(h);}if(other)snapshot(other);heif_image_handle_release(h);heif_image_handle_release(alias);heif_image_handle_release(other);heif_image_handle_release(newer);
 printf(" null%d",heif_image_handle_has_gimi_component_content_ids(NULL));string(heif_image_handle_get_gimi_component_content_id(NULL,0));heif_image_handle_set_gimi_component_content_id(NULL,0,"unused");puts("");
 }return ferror(stdout)?1:0;}
