/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_components.h>
#include <libheif/heif_properties.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <inttypes.h>
#include <string.h>
static void error(heif_error e){printf("E%d/%d/%s;",e.code,e.subcode,e.message?e.message:"NULL");}
static void description(const heif_image_handle* h){
 uint32_t ids[34];for(unsigned i=0;i<34;i++)ids[i]=0xabcdef01;uint32_t n=heif_image_handle_get_number_of_components(h);if(n>32)exit(4);
 heif_image_handle_get_used_component_ids(h,ids+1);heif_image_handle_get_used_component_ids(h,NULL);printf("H%u/%u/%u;",n,ids[0],ids[n+1]);
 for(unsigned i=0;i<n+2;i++){uint32_t id=i<n?ids[i+1]:(i==n?0:UINT32_MAX);printf("%u:%u,%d,%d;",id,heif_image_handle_get_component_type(h,id),heif_image_handle_get_component_bits_per_pixel(h,id),heif_image_handle_get_component_datatype(h,id));}
}
static void pixels(heif_image* p){
 uint32_t ids[34];for(unsigned i=0;i<34;i++)ids[i]=0xabcdef01;uint32_t n=heif_image_get_number_of_used_components(p);if(n>32)exit(5);heif_image_get_used_component_ids(p,ids+1);printf("P%u/%u/%u;",n,ids[0],ids[n+1]);
 for(unsigned i=0;i<n;i++){
 uint32_t id=ids[i+1],w=heif_image_get_component_width(p,id),h=heif_image_get_component_height(p,id);int b=heif_image_get_component_bits_per_pixel(p,id),ch=heif_image_get_component_channel(p,id);
 size_t stride=0;const uint8_t* data=heif_image_get_component_readonly(p,id,&stride);printf("%u:%u,%u,%u,%d,%d,%d,%zu,%d;",id,heif_image_get_component_type(p,id),w,h,b,ch,heif_image_get_component_datatype(p,id),stride,data!=NULL);
 if(data){uint64_t hash=1469598103934665603ULL;unsigned bytes=b<=8?1:b<=16?2:b<=32?4:b<=64?8:16;for(uint32_t y=0;y<h;y++)for(size_t x=0;x<(size_t)w*bytes;x++)hash=(hash^data[y*stride+x])*1099511628211ULL;printf("hash=%" PRIx64 ";",hash);
 for(unsigned j=0;j<i;j++)printf("alias%d;",heif_image_get_component_readonly(p,ids[j+1],NULL)==data);
 }
 }
 uint32_t id=123;error(heif_image_add_bayer_component(p,6,&id));printf("next=%u;",id);
}
int main(void){
 uint32_t header[2];unsigned index=0;
 while(fread(header,sizeof(header),1,stdin)==1){uint32_t length=header[0],mode=header[1];if(length>4000000)exit(2);uint8_t* data=malloc(length+1);if(fread(data,1,length,stdin)!=length)exit(3);
 printf("case%u;",index++);heif_context* ctx=heif_context_alloc();error(heif_context_read_from_memory(ctx,data,length,NULL));
 uint32_t ids[50];int n=heif_context_get_list_of_top_level_image_IDs(ctx,ids,32);for(uint32_t i=1;i<=12;i++){int found=0;for(int j=0;j<n;j++)found|=ids[j]==i;if(!found)ids[n++]=i;}
 heif_image_handle* retained=NULL;heif_image* decoded=NULL;
 for(int i=0;i<n;i++){heif_image_handle* h=NULL;heif_error e=heif_context_get_image_handle(ctx,ids[i],&h);printf("id%u;",ids[i]);error(e);if(e.code)continue;
 description(h);heif_colorspace cs=99;heif_chroma ch=99;error(heif_image_handle_get_preferred_decoding_colorspace(h,&cs,&ch));printf("cs=%d/%d;",cs,ch);printf("bits=%d,%d;",heif_image_handle_get_luma_bits_per_pixel(h),heif_image_handle_get_chroma_bits_per_pixel(h));
 if(mode && i==0){heif_decoding_options* opts=heif_decoding_options_alloc();opts->color_conversion_options.preferred_chroma_downsampling_algorithm=heif_chroma_downsampling_nearest_neighbor;opts->ignore_transformations=mode==1;opts->color_conversion_options.only_use_preferred_chroma_algorithm=0;heif_context_set_max_decoding_threads(ctx,0);
 e=heif_decode_image(h,&decoded,mode==3?heif_colorspace_RGB:heif_colorspace_undefined,mode==3?heif_chroma_interleaved_RGBA:heif_chroma_undefined,opts);error(e);heif_decoding_options_free(opts);if(!e.code)pixels(decoded);}
 if(!retained)retained=h;else heif_image_handle_release(h);
 }
 error(heif_context_read_from_memory(ctx,"x",1,NULL));description(retained);
 uint8_t* changed=malloc(length+1);memcpy(changed,data,length);for(size_t j=0;j+23<=length;j++)if(!memcmp(changed+j,"hvcC",4)){changed[j+20]=0;changed[j+21]=7;break;}
 error(heif_context_read_from_memory(ctx,changed,length,NULL));memset(changed,0,length);free(changed);description(retained);
 heif_image_handle* newer=NULL;heif_error re=heif_context_get_primary_image_handle(ctx,&newer);error(re);if(!re.code){description(newer);heif_image_handle_release(newer);}
 heif_context_free(ctx);description(retained);heif_image_handle_release(retained);free(data);
 if(decoded){pixels(decoded);heif_image_release(decoded);}puts("");
 }
 printf("null:");description(NULL);puts("");return 0;
}
