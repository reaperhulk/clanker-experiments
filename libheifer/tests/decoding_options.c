/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int user;
static void progress(heif_progress_step step, int n, void* p) {(void)step;(void)n;(void)p;}
static void end(heif_progress_step step, void* p) {(void)step;(void)p;}
static int cancel(void* p) {(void)p;return 0;}
static heif_color_conversion_options_ext ext;
static heif_color_profile_nclx profile;
static const char decoder[]="test-decoder";
static heif_decoding_options source(void) {
  heif_decoding_options s={0};
  s.version=10;s.ignore_transformations=31;s.start_progress=progress;s.on_progress=progress;
  s.end_progress=end;s.progress_user_data=&user;s.convert_hdr_to_8bit=32;s.strict_decoding=33;
  s.decoder_id=decoder;s.color_conversion_options.version=34;
  s.color_conversion_options.preferred_chroma_downsampling_algorithm=3;
  s.color_conversion_options.preferred_chroma_upsampling_algorithm=1;
  s.color_conversion_options.only_use_preferred_chroma_algorithm=35;
  s.cancel_decoding=cancel;s.color_conversion_options_ext=&ext;s.ignore_sequence_editlist=-36;
  s.output_image_nclx_profile=&profile;s.num_library_threads=-37;s.num_codec_threads=38;
  s.autocorrect_broken_input=39;s.output_image_nclx_profile_passthrough=40;
  return s;
}
static void dump(const heif_decoding_options* o) {
  printf("%u %u %d %d %d %d %u %u %d %u %d %d %u %d %d %d %d %d %d %u %u\n",
    o->version,o->ignore_transformations,o->start_progress==progress,o->on_progress==progress,
    o->end_progress==end,o->progress_user_data==&user,o->convert_hdr_to_8bit,o->strict_decoding,
    o->decoder_id==decoder,o->color_conversion_options.version,
    o->color_conversion_options.preferred_chroma_downsampling_algorithm,
    o->color_conversion_options.preferred_chroma_upsampling_algorithm,
    o->color_conversion_options.only_use_preferred_chroma_algorithm,o->cancel_decoding==cancel,
    o->color_conversion_options_ext==&ext,o->ignore_sequence_editlist,
    o->output_image_nclx_profile==&profile,o->num_library_threads,o->num_codec_threads,
    o->autocorrect_broken_input,o->output_image_nclx_profile_passthrough);
}
int main(void) {
  heif_decoding_options* defaults=heif_decoding_options_alloc();dump(defaults);
  heif_decoding_options_copy(defaults,NULL);dump(defaults);
  heif_decoding_options_free(defaults);heif_decoding_options_free(NULL);
  for(unsigned d=0;d<256;d++) for(unsigned s=0;s<256;s++) {
    heif_decoding_options* dst=heif_decoding_options_alloc();dst->version=d;
    heif_decoding_options src=source();src.version=s;
    heif_decoding_options_copy(dst,&src);dump(dst);heif_decoding_options_free(dst);
  }
  /* Exact old-struct prefixes catch accidental whole-struct reads/writes under ASan. */
  size_t sizes[]={1,offsetof(heif_decoding_options,convert_hdr_to_8bit),
    offsetof(heif_decoding_options,strict_decoding),offsetof(heif_decoding_options,decoder_id),
    offsetof(heif_decoding_options,color_conversion_options),offsetof(heif_decoding_options,cancel_decoding),
    offsetof(heif_decoding_options,color_conversion_options_ext),offsetof(heif_decoding_options,ignore_sequence_editlist),
    offsetof(heif_decoding_options,autocorrect_broken_input),offsetof(heif_decoding_options,output_image_nclx_profile_passthrough),
    sizeof(heif_decoding_options)};
  for(unsigned version=0;version<=10;version++) {
    heif_decoding_options src=source();src.version=version;
    heif_decoding_options* prefix=malloc(sizes[version]);memcpy(prefix,&src,sizes[version]);
    heif_decoding_options* dst=heif_decoding_options_alloc();
    heif_decoding_options_copy(dst,prefix);dump(dst);
    heif_decoding_options_copy(prefix,&src);heif_decoding_options_copy(prefix,prefix);
    free(prefix);heif_decoding_options_free(dst);
  }
  return 0;
}
