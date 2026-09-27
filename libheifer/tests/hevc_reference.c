/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static void number(FILE* out, uint32_t n) {
  for (int i = 0; i < 4; ++i) { fputc(n & 255, out); n >>= 8; }
}
static void check(heif_error err) {
  if (err.code) { fprintf(stderr, "%d:%d:%s\n", err.code, err.subcode, err.message); exit(2); }
}
int main(int argc, char** argv) {
  if (argc != 3 && argc != 4) return 1;
  heif_context* ctx = heif_context_alloc();
  check(heif_context_read_from_file(ctx, argv[1], NULL));
  int count = heif_context_get_number_of_top_level_images(ctx);
  heif_item_id* ids = calloc((size_t)count, sizeof(*ids));
  heif_context_get_list_of_top_level_image_IDs(ctx, ids, count);
  FILE* out = fopen(argv[2], "wb");
  if (!out) return 1;
  number(out, count);
  for (int i = 0; i < count; ++i) {
    heif_image_handle* handle = NULL;
    check(heif_context_get_image_handle(ctx, ids[i], &handle));
    heif_decoding_options* options = heif_decoding_options_alloc();
    options->ignore_transformations = 1;
    options->output_image_nclx_profile_passthrough = argc == 4;
    heif_image* image = NULL;
    check(heif_decode_image(handle, &image, heif_colorspace_undefined, heif_chroma_undefined, options));
    number(out, ids[i]); number(out, heif_image_get_primary_width(image));
    number(out, heif_image_get_primary_height(image)); number(out, heif_image_get_colorspace(image));
    number(out, heif_image_get_chroma_format(image));
    for (int channel = 0; channel < 3; ++channel) {
      if (!heif_image_has_channel(image, channel)) { for(int n=0; n<5; ++n) number(out, 0); continue; }
      uint32_t w = heif_image_get_width(image, channel), h = heif_image_get_height(image, channel);
      uint32_t bits = heif_image_get_bits_per_pixel(image, channel);
      size_t stride;
      const uint8_t* p = heif_image_get_plane_readonly2(image, channel, &stride);
      number(out, w); number(out, h); number(out, heif_image_get_bits_per_pixel_range(image, channel));
      number(out, bits); number(out, stride);
      for (uint32_t y=0; y<h; ++y) fwrite(p + y * stride, 1, w * (bits / 8), out);
    }
    heif_image_release(image); heif_decoding_options_free(options); heif_image_handle_release(handle);
  }
  free(ids); fclose(out); heif_context_free(ctx);
  return 0;
}
