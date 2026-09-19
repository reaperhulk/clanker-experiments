/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define main context_original_main
#include "context.c"
#undef main
#include <stddef.h>
#include <libheif/heif_items.h>
struct source { unsigned char* data; size_t length, position; unsigned flags; int status; unsigned issued, freed, second_reads, invalid; };
static struct source* active;
static int64_t position(void* u) { return ((struct source*)u)->position; }
static int read_data(void* p,size_t n,void* u) { struct source* s=u;if(s->position>s->length||n>s->length-s->position)return 1;memcpy(p,s->data+s->position,n);s->position+=n;return 0; }
static int read_second(void* p,size_t n,void* u) { ((struct source*)u)->second_reads++;return read_data(p,n,u); }
static int seek_data(int64_t p,void* u) { struct source* s=u;if(p<0||(uint64_t)p>s->length)return 1;s->position=(size_t)p;return 0; }
static enum heif_reader_grow_status wait_data(int64_t p,void* u) { struct source* s=u;return p>=0&&(uint64_t)p<=s->length?heif_reader_grow_status_size_reached:((s->flags&16)?heif_reader_grow_status_timeout:heif_reader_grow_status_size_beyond_eof); }
static heif_reader_range_request_result request(uint64_t start,uint64_t end,void* u) { struct source* s=u;heif_reader_range_request_result r={end<=s->length?heif_reader_grow_status_size_reached:heif_reader_grow_status_size_beyond_eof,s->length,43,NULL};if(start>end)s->invalid++;if((s->flags&1)||((s->flags&64)&&start>=32))r.status=s->status;if(s->flags&8){char* m=malloc(15);memcpy(m,"caller failure",15);r.reader_error_msg=m;s->issued++;}return r; }
static void range_hint(uint64_t start,uint64_t end,void* u) { if(start>end)((struct source*)u)->invalid++; }
static void release_message(const char* p) { active->freed++;free((void*)p); }
static void pixels(heif_image_handle* handle) { heif_image* img=(void*)(uintptr_t)0x1234;heif_error e=heif_decode_image(handle,&img,heif_colorspace_undefined,heif_chroma_undefined,NULL);err(e);printf(" decoded=%d:%d",img==NULL,img==(void*)(uintptr_t)0x1234);if(!e.code){int stride;const uint8_t* p=heif_image_get_plane_readonly(img,heif_channel_Y,&stride);int w=heif_image_get_width(img,heif_channel_Y),h=heif_image_get_height(img,heif_channel_Y),b=heif_image_get_bits_per_pixel(img,heif_channel_Y);printf(" plane=%d:%d:%d:",w,h,b);for(int y=0;y<h;y++)for(int x=0;x<w*((b+7)/8);x++)printf("%02x",p[y*stride+x]);heif_image_release(img);} }
int main(void) { uint32_t args[4];while(fread(args,sizeof(args),1,stdin)==1){size_t length=args[0];if(length>2000000)return 2;struct source s={0};s.length=length;s.flags=args[2];s.status=(int)args[3];s.data=malloc(length+1);if(fread(s.data,1,length,stdin)!=length)return 2;active=&s;
 size_t table_size=args[1]<2?offsetof(heif_reader,request_range):sizeof(heif_reader);heif_reader* reader=calloc(1,table_size);reader->reader_api_version=args[1];reader->get_position=position;reader->read=read_data;reader->seek=seek_data;reader->wait_for_file_size=wait_data;if(args[1]>=2){reader->request_range=request;reader->preload_range_hint=range_hint;reader->release_file_range=range_hint;reader->release_error_msg=release_message;}
 heif_context* ctx=heif_context_alloc();err(heif_context_read_from_reader(ctx,reader,&s,NULL));printf(" count=%d items=%d",heif_context_get_number_of_top_level_images(ctx),(int)heif_context_get_number_of_items(ctx));uint32_t id=0x12345678;err(heif_context_get_primary_image_ID(ctx,&id));printf(" id=%u",id);heif_image_handle* h=(void*)(uintptr_t)0x1234;heif_error e=heif_context_get_primary_image_handle(ctx,&h);err(e);printf(" out=%d:%d",h==NULL,h==(void*)(uintptr_t)0x1234);
 int mask=0;for(size_t i=0;i+4<=length;i++)if(!memcmp(s.data+i,"mskC",4))mask=1;
 reader->read=read_second;if(s.flags&32)for(size_t i=0;i+9<=length;i++)if(!memcmp(s.data+i,"idat",4)){for(size_t j=0;j<5;j++)s.data[i+4+j]^=0x5a;break;}
 heif_context_free(ctx);if(!e.code){image(h);if(mask)pixels(h);heif_image_handle_release(h);}printf(" callback-invariants=%d:%d late-read=%d",s.invalid==0,s.issued==s.freed,s.second_reads!=0);free(reader);free(s.data);puts("");}return ferror(stdin)?2:0;}
