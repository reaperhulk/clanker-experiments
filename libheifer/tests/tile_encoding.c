/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main encoding_main
#define output encoding_output
#include "encoding.c"
#undef main
#undef output
#include <libheif/heif_tiling.h>
static heif_error output(heif_context* ctx,const void* data,size_t size,void* u) {
  (void)ctx;(void)u;printf(" file%zu:",size);for(size_t i=0;i<size;i++)printf("%02x",((const uint8_t*)data)[i]);
  heif_context* read=heif_context_alloc();heif_context_set_max_decoding_threads(read,0);heif_error e=heif_context_read_from_memory(read,data,size,NULL);error(e);
  if(!e.code){heif_image_handle* h=NULL;error(heif_context_get_primary_image_handle(read,&h));if(h){inspect(h,1);heif_image_handle_release(h);}}
  heif_context_free(read);return (heif_error){0,0,"Success"};
}
static void geometry(heif_image_handle* h) {
  printf(" geom%d,%d,%d,%d,%d",heif_image_handle_get_width(h),heif_image_handle_get_height(h),heif_image_handle_get_ispe_width(h),heif_image_handle_get_ispe_height(h),heif_image_handle_is_primary_image(h));
  for(int process=0;process<2;process++) {heif_image_tiling t;memset(&t,0xA5,sizeof t);error(heif_image_handle_get_image_tiling(h,process,&t));printf(" tiling%d,%u,%u,%u,%u,%u,%u,%u,%u",t.version,t.num_columns,t.num_rows,t.tile_width,t.tile_height,t.image_width,t.image_height,t.top_offset,t.left_offset);}
}
static heif_image* tile(uint32_t* v,unsigned i) {
  uint32_t local[10];memcpy(local,v,sizeof local);v=local;if(i==1&&(v[8]&2048))v[9]=v[9]==8?16:8;if(i==1&&(v[8]&65536))v[4]++;
  heif_image* img=NULL;int rgb=!!(v[8]&16384),space=rgb?1:2,chroma=rgb?10:0;error(heif_image_create(v[4],v[5],(heif_colorspace)space,(heif_chroma)chroma,&img));
  if(!img)return NULL;int ch=rgb?10:0;error(heif_image_add_plane(img,(heif_channel)ch,v[4],v[5],v[9]));int stride=0;uint8_t* p=heif_image_get_plane(img,(heif_channel)ch,&stride);
  if(p)for(unsigned y=0;y<v[5];y++)for(unsigned x=0;x<v[4]*(rgb?3:1)*(v[9]>8?2:1);x++)p[y*stride+x]=(uint8_t)(i*71+y*29+x*13);
  if(v[8]&8){heif_image_set_pixel_aspect_ratio(img,3,2);heif_content_light_level clli={123,17};heif_image_set_content_light_level(img,&clli);heif_image_set_gimi_sample_content_id(img,"tile content");heif_image_set_omaf_image_projection(img,heif_omaf_image_projection_equirectangular);heif_color_profile_nclx* n=heif_nclx_color_profile_alloc();n->matrix_coefficients=6;error(heif_image_set_nclx_color_profile(img,n));heif_nclx_color_profile_free(n);}
  return img;
}
int main(void) {
  setvbuf(stdout,NULL,_IONBF,0);uint32_t v[10];
  while(fread(v,sizeof v,1,stdin)==1) {
    heif_context* ctx=heif_context_alloc();heif_encoder* enc=NULL;error(heif_context_get_encoder_for_format(ctx,(heif_compression_format)v[0],&enc));
    heif_encoding_options* opts=heif_encoding_options_alloc();opts->image_orientation=(heif_orientation)v[6];
    heif_unci_image_parameters* p=heif_unci_image_parameters_alloc();p->image_width=v[4]*v[2]+!!(v[8]&32);p->image_height=v[5]*v[3];p->tile_width=v[4];p->tile_height=v[5];p->compression=(heif_unci_compression)v[7];opts->unci_parameters=p;
    if(v[8]&256)opts->version=7;
    heif_image_handle* h=(void*)(uintptr_t)1;heif_image* images[64]={0};unsigned count=v[2]*v[3];if(count>64)count=0;
    for(unsigned i=0;i<count;i++)images[i]=tile(v,i);
    heif_encoding_options* o=v[8]&16?NULL:opts;heif_encoder* e=v[8]&1024?NULL:enc;heif_image_handle** out=v[8]&512?NULL:&h;
    if(v[1]==0)error(heif_context_add_grid_image(ctx,p->image_width,p->image_height,v[2],v[3],o,out));
    else if(v[1]==1)error(heif_context_encode_grid(ctx,images,v[2],v[3],e,o,out));
    else error(heif_context_add_empty_unci_image(ctx,v[8]&64?NULL:p,o,images[0],out));
    printf(" out%d items%d",h==NULL?0:h==(void*)(uintptr_t)1?1:2,heif_context_get_number_of_items(ctx));
    if(h&&h!=(void*)(uintptr_t)1) {
      geometry(h);
      if(v[1]!=1) {
        for(unsigned j=0;j<count;j++) {unsigned i=v[8]&1?count-1-j:j;if((v[8]&4)&&i==count-1)continue;if((v[8]&128)&&i==0)continue;error(heif_context_add_image_tile(ctx,h,i%v[2],i/v[2],images[i],e));}
        if((v[8]&2)&&count)error(heif_context_add_image_tile(ctx,h,0,0,images[count-1],e));
        if(v[8]&4096)error(heif_context_add_image_tile(ctx,h,v[2],0,images[0],e));
        geometry(h);
      }
      heif_writer wr={1,output};error(heif_context_write(ctx,&wr,NULL));if(v[8]&8192)error(heif_context_write(ctx,&wr,NULL));heif_image_handle_release(h);
    }
    for(unsigned i=0;i<count;i++)if(images[i])heif_image_release(images[i]);heif_unci_image_parameters_release(p);heif_encoding_options_free(opts);if(enc)heif_encoder_release(enc);heif_context_free(ctx);puts("");
  }
  return 0;
}
