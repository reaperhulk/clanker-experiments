/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Test-only caller plugin; never linked into the Rust candidate. */
#include <libheif/heif_plugin.h>
#include <stdio.h>
static int (*query)(void);
static const char* name(void){return "dynamic probe";}
static void init(void){printf(" I%d",query?query():-1);}
static void cleanup(void){printf(" C%d",query?query():-1);}
static int support(heif_compression_format f){return (int)f==99?71:0;}
static heif_encoder_plugin encoder={.plugin_api_version=4,.compression_format=(heif_compression_format)99,.id_name="dynamic-encoder",.priority=71,.get_plugin_name=name,.init_plugin=init,.cleanup_plugin=cleanup};
static heif_decoder_plugin decoder={.plugin_api_version=6,.id_name="dynamic-decoder",.get_plugin_name=name,.init_plugin=init,.deinit_plugin=cleanup,.does_support_format=support};
heif_plugin_info plugin_info={1,heif_plugin_type_encoder,&encoder,NULL};
void configure(int kind,int version,unsigned minimum,int info_version,int (*count)(void)){
 query=count;encoder.plugin_api_version=version;encoder.minimum_required_libheif_version=minimum;decoder.plugin_api_version=version;decoder.minimum_required_libheif_version=minimum;plugin_info.version=info_version;plugin_info.type=(heif_plugin_type)kind;plugin_info.plugin=kind==1?(void*)&decoder:(void*)&encoder;
}
