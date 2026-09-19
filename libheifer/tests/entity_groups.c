/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <libheif/heif_entity_groups.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static void error(heif_error e){printf(" e%d/%d/",e.code,e.subcode);if(e.message)for(const unsigned char* p=(const unsigned char*)e.message;*p;p++)printf("%02x",*p);}
static void show(heif_entity_group* g,int count){printf(" groups%d/%d",count,g==NULL);for(int i=0;i<count;i++){printf(" g%u/%u/%u/%d",g[i].entity_group_id,g[i].entity_group_type,g[i].num_entities,g[i].entities==NULL);if(g[i].num_entities>10000)abort();for(unsigned j=0;j<g[i].num_entities;j++)printf("/%u",g[i].entities[j]);}}
static heif_entity_group* snapshot(heif_context* ctx,int* retained_count){
 uint32_t types[]={0,heif_entity_group_altr,heif_entity_group_pymd,heif_entity_group_eqiv,heif_entity_group_brst,heif_entity_group_tsyn,heif_entity_group_ster,heif_entity_group_stem,heif_entity_group_aebr,heif_entity_group_wbbr,heif_entity_group_fobr,heif_entity_group_afbr,heif_entity_group_dobr,heif_entity_group_albc,heif_entity_group_favc,heif_entity_group_pano,heif_entity_group_slid,heif_entity_group_prgr,0xffffffff};
 uint32_t ids[]={0,1,2,3,7,0xffffffff};
 for(unsigned t=0;t<sizeof(types)/sizeof(*types);t++)for(unsigned i=0;i<sizeof(ids)/sizeof(*ids);i++){
  int count=-999;heif_entity_group* groups=heif_context_get_entity_groups(ctx,types[t],ids[i],&count);printf(" filter%u/%u",types[t],ids[i]);show(groups,count);heif_entity_groups_release(groups,count);
 }
 return heif_context_get_entity_groups(ctx,0,0,retained_count);
}
int main(void){uint32_t v[4];unsigned number=0;while(fread(v,sizeof(v),1,stdin)==1){
 uint8_t* data=malloc(v[0]+1),*reload=malloc(v[1]+1);if(fread(data,1,v[0],stdin)!=v[0]||fread(reload,1,v[1],stdin)!=v[1])return 2;
 heif_context* ctx=heif_context_alloc();printf("case%u",number++);int n=0;heif_entity_group* fresh=snapshot(ctx,&n);show(fresh,n);heif_entity_groups_release(fresh,n);
 if(v[2]&1)heif_context_get_security_limits(ctx)->max_size_entity_group=v[3];if(v[2]&2)heif_context_get_security_limits(ctx)->max_children_per_box=v[3];
 error(heif_context_read_from_memory(ctx,data,v[0],NULL));free(data);int first_count=-11;heif_entity_group* first=snapshot(ctx,&first_count);
 // Caller mutations of snapshots must never alter the context or other copies.
 int copy_count=-12;heif_entity_group* copy=heif_context_get_entity_groups(ctx,0,0,&copy_count);
 if(first_count){first[0].entity_group_id^=0xabcdef;if(first[0].num_entities)first[0].entities[0]^=0x123456;}
 show(copy,copy_count);heif_entity_groups_release(copy,copy_count);
 error(heif_context_read_from_memory(ctx,reload,v[1],NULL));free(reload);int second_count=-13;heif_entity_group* second=snapshot(ctx,&second_count);show(first,first_count);
 heif_context_free(ctx);show(first,first_count);show(second,second_count);heif_entity_groups_release(first,first_count);heif_entity_groups_release(second,second_count);heif_entity_groups_release(NULL,0);puts("");
 }return ferror(stdout)?1:0;}
