/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define error transforms_error
#define main unused_transforms_main
#include "transforms.c"
#undef main
#undef error
static void error(heif_error e){printf(" e=%d,%d,",e.code,e.subcode);if(e.message){for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}else printf("NULL");}
#include <libheif/heif_regions.h>
static void query(heif_region* r){
 int type=heif_region_get_type(r);printf(" region%d",type);
 int32_t x=-777,y=-888;uint32_t w=123456,h=654321,id=999;
 error(heif_region_get_point(r,&x,&y));printf(" point%d/%d",x,y);
 error(heif_region_get_point(r,NULL,&y));
 error(heif_region_get_rectangle(r,&x,&y,&w,&h));printf(" rect%d/%d/%u/%u",x,y,w,h);
 error(heif_region_get_ellipse(r,&x,&y,&w,&h));printf(" ellipse%d/%d/%u/%u",x,y,w,h);
 error(heif_region_get_referenced_mask_ID(r,&x,&y,&w,&h,&id));printf(" ref%d/%d/%u/%u/%u",x,y,w,h,id);
 error(heif_region_get_referenced_mask_ID(r,&x,&y,&w,&h,NULL));
 int n=heif_region_get_polygon_num_points(r),n2=heif_region_get_polyline_num_points(r);printf(" counts%d/%d",n,n2);if(n<0||n>10000)abort();
 int32_t* points=malloc((2*n+2)*sizeof(*points));double* scaled=malloc((2*n+2)*sizeof(*scaled));
 for(int poly=0;poly<2;poly++){
  for(int j=0;j<2*n+2;j++)points[j]=-991;
  error(poly?heif_region_get_polygon_points(r,points):heif_region_get_polyline_points(r,points));
  for(int j=0;j<2*n+2;j++)printf("/%d",points[j]);
  error(poly?heif_region_get_polygon_points(r,NULL):heif_region_get_polyline_points(r,NULL));
 }
 for(int target=0;target<3;target++){
  id=target==2?999:target+1;double a=-10,b=-11,c=-12,d=-13;
  error(heif_region_get_point_transformed(r,id,&a,&b));printf(" tp%a/%a",a,b);
  error(heif_region_get_point_transformed(r,id,NULL,&b));
  error(heif_region_get_rectangle_transformed(r,id,&a,&b,&c,&d));printf(" tr%a/%a/%a/%a",a,b,c,d);
  error(heif_region_get_ellipse_transformed(r,id,&a,&b,&c,&d));printf(" te%a/%a/%a/%a",a,b,c,d);
  for(int poly=0;poly<2;poly++){
   for(int j=0;j<2*n+2;j++)scaled[j]=-991;
   error(poly?heif_region_get_polygon_points_transformed(r,id,scaled):heif_region_get_polyline_points_transformed(r,id,scaled));
   for(int j=0;j<2*n+2;j++)printf("/%a",scaled[j]);
   error(poly?heif_region_get_polygon_points_transformed(r,id,NULL):heif_region_get_polyline_points_transformed(r,id,NULL));
  }
 }
 free(points);free(scaled);
 size_t len=heif_region_get_inline_mask_data_len(r);printf(" len%zu",len);if(len>100000)abort();uint8_t* data=malloc(len+2);memset(data,0x59,len+2);
 error(heif_region_get_inline_mask_data(r,&x,&y,&w,&h,data+1));printf(" inline%d/%d/%u/%u:",x,y,w,h);for(size_t i=0;i<len+2;i++)printf("%02x",data[i]);free(data);
 error(heif_region_get_inline_mask_data(r,NULL,&y,&w,&h,NULL));
 heif_image* image=(void*)0x1234;heif_error e=heif_region_get_mask_image(r,&x,&y,&w,&h,&image);error(e);printf(" mask%d/%d/%u/%u/%d/%d",x,y,w,h,image==NULL,image==(void*)0x1234);if(!e.code){dump(image);heif_image_release(image);}
}
static heif_region* item_query(heif_region_item* item){
 uint32_t w=17,h=19;heif_region_item_get_reference_size(item,&w,&h);printf(" item%u/%u/%u/%d",heif_region_item_get_id(item),w,h,heif_region_item_get_number_of_regions(item));
 heif_region_item_get_reference_size(item,NULL,NULL);heif_region_item_get_reference_size(item,&w,NULL);heif_region_item_get_reference_size(item,NULL,&h);
 int n=heif_region_item_get_number_of_regions(item);if(n<0||n>10000)abort();
 for(int capacity=-1;capacity<=n+1;capacity+=(capacity<2?1:3)){
  heif_region** rs=malloc((n+3)*sizeof(*rs));for(int j=0;j<n+3;j++)rs[j]=(void*)0x1234;
  int used=heif_region_item_get_list_of_regions(item,rs+1,capacity);printf(" list%d/%d",capacity,used);for(int j=0;j<n+3;j++)printf("/%d",rs[j]==(void*)0x1234);
  if(used>0)heif_region_release_many((const heif_region* const*)(rs+1),used);free(rs);
 }
 heif_region** rs=calloc(n+1,sizeof(*rs));int used=heif_region_item_get_list_of_regions(item,rs,n+1);for(int j=0;j<used;j++)query(rs[j]);
 heif_region* saved=used?rs[0]:NULL;for(int j=1;j<used;j++)heif_region_release(rs[j]);free(rs);return saved;
}
static void ids(heif_image_handle* handle){
 int n=heif_image_handle_get_number_of_region_items(handle);printf(" ids%d",n);
 for(int c=0;c<5;c++){uint32_t ids[7];for(int j=0;j<7;j++)ids[j]=999;int used=heif_image_handle_get_list_of_region_item_ids(handle,ids+1,c);printf(" ids%d/%d",c,used);for(int j=0;j<7;j++)printf("/%u",ids[j]);}
}
int main(void){uint32_t v[8];unsigned number=0;while(fread(v,sizeof(v),1,stdin)==1){
 uint8_t* file=malloc(v[0]+1),*reload=malloc(v[1]+1);if(fread(file,1,v[0],stdin)!=v[0]||fread(reload,1,v[1],stdin)!=v[1])return 2;
 heif_context* ctx=heif_context_alloc();if(v[3]&4)heif_context_get_security_limits(ctx)->max_memory_block_size=v[7];if(v[3]&8)heif_context_get_security_limits(ctx)->max_image_size_pixels=v[7];if(v[3]&16)heif_context_get_security_limits(ctx)->max_total_memory=v[7];
 printf("case%u",number++);error(heif_context_read_from_memory(ctx,file,v[0],NULL));free(file);printf(" top%d",heif_context_get_number_of_top_level_images(ctx));
 heif_image_handle* handle=NULL;error(heif_context_get_primary_image_handle(ctx,&handle));if(handle)ids(handle);
 heif_region_item* old=NULL;heif_region* retained=NULL;
 for(uint32_t id=0;id<6;id++){heif_region_item* item=(void*)0x1234;heif_error e=heif_context_get_region_item(ctx,id,&item);error(e);printf(" lookup%u/%d",id,item==(void*)0x1234);if(!e.code){heif_region* r=item_query(item);if(!old){old=item;retained=r;}else{heif_region_item_release(item);heif_region_release(r);}}}
 error(heif_context_get_region_item(ctx,1,NULL));
 heif_region_item* added=NULL;
 if(handle && !(v[3]&1)){
  error(heif_image_handle_add_region_item(handle,v[4],v[5],&added));ids(handle);
  if(added){
   int32_t x=(int32_t)v[2],y=(int32_t)(v[2]^0xabcdef01);heif_region* r=NULL;
   error(heif_region_item_add_region_point(added,x,y,&r));query(r);heif_region_release(r);
   error(heif_region_item_add_region_rectangle(added,x,y,v[4],v[5],&r));heif_region_release(r);
   error(heif_region_item_add_region_ellipse(added,y,x,v[5],v[4],NULL));
   int32_t pts[]={x,y,-3,7,1,-11,991,-997};
   for(int poly=0;poly<2;poly++)for(int n=-1;n<5;n++){
    r=(void*)0x1234;heif_error e=poly?heif_region_item_add_region_polygon(added,pts,n,&r):heif_region_item_add_region_polyline(added,pts,n,&r);error(e);printf(" out%d/%d",r==NULL,r==(void*)0x1234);if(!e.code)heif_region_release(r);
   }
   memset(pts,0xff,sizeof(pts));
   r=(void*)0x1234;error(heif_region_item_add_region_polygon(added,NULL,1,&r));printf(" out%d",r==NULL);error(heif_region_item_add_region_polyline(added,NULL,0,NULL));
   error(heif_region_item_add_region_referenced_mask(added,x,y,0,0,2,&r));heif_region_release(r);
   error(heif_region_item_add_region_referenced_mask(added,x,y,3,7,999,NULL));
   uint32_t mw=v[6]%17+1,mh=(v[6]/17)%13+1;size_t len=(mw*mh+7)/8;uint8_t* data=malloc(len+1);for(size_t j=0;j<len+1;j++)data[j]=(v[2]+j*113)&255;
   for(int delta=-1;delta<=1;delta++){r=(void*)0x1234;heif_error e=heif_region_item_add_region_inline_mask_data(added,x,y,mw,mh,data,len+delta,&r);error(e);printf(" out%d/%d",r==NULL,r==(void*)0x1234);if(!e.code){query(r);heif_region_release(r);}}
   error(heif_region_item_add_region_inline_mask_data(added,x,y,0,mh,data,len,&r));error(heif_region_item_add_region_inline_mask_data(added,x,y,mw,0,data,len,&r));error(heif_region_item_add_region_inline_mask_data(added,x,y,mw,mh,NULL,len,&r));memset(data,0,len+1);free(data);
   heif_image* mask=create(v[6]%9+1,(v[6]/9)%7+1,2,0,v[3]&2?16:8,0,0);error(heif_region_item_add_region_inline_mask(added,x,y,mw,mh,mask,&r));heif_image_release(mask);heif_region_release(r);
   heif_image* rgb=create(2,2,1,3,8,0,0);r=(void*)0x1234;error(heif_region_item_add_region_inline_mask(added,x,y,mw,mh,rgb,&r));printf(" noY%d",r==(void*)0x1234);heif_image_release(rgb);
   heif_region* saved=item_query(added);heif_region_release(saved);
  }
 }
 error(heif_context_read_from_memory(ctx,reload,v[1],NULL));free(reload);if(handle)ids(handle);
 if(old){heif_region* r=item_query(old);heif_region_release(r);}if(added){heif_region* r=item_query(added);heif_region_release(r);}
 heif_context_free(ctx);heif_image_handle_release(handle);heif_region_item_release(old);heif_region_item_release(added);
 if(retained){query(retained);heif_region_release(retained);}heif_region_release(NULL);heif_region_item_release(NULL);printf(" nullid%u",heif_region_item_get_id(NULL));puts("");
 }return ferror(stdout)?1:0;}
