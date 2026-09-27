/* SPDX-License-Identifier: LGPL-3.0-or-later */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static void err(struct heif_error e) {
  printf(" %d:%d:%s", e.code, e.subcode, e.message ? e.message : "NULL");
}
static void inspect(heif_image* image, int channel, int fill) {
  int stride = -99;
  size_t wide = 123;
  uint8_t* data = heif_image_get_plane(image, channel, &stride);
  uint8_t* data2 = heif_image_get_plane2(image, channel, &wide);
  printf(" |%d:%d:%d:%d:%d:%d:%d:%zu:%d", channel,
         heif_image_has_channel(image, channel), heif_image_get_width(image, channel),
         heif_image_get_height(image, channel), heif_image_get_bits_per_pixel(image, channel),
         heif_image_get_bits_per_pixel_range(image, channel), stride, wide, data == data2);
  if (data) {
    int w = heif_image_get_width(image, channel);
    int h = heif_image_get_height(image, channel);
    uint64_t zero = 0;
    for (size_t i = 0; i < wide * (size_t)h; ++i) zero |= data[i];
    printf(":align%zu:zero%llu", (size_t)((uintptr_t)data % 16), (unsigned long long)zero);
    if (fill) {
      for (int y = 0; y < h; ++y) for (int x = 0; x < w; ++x)
        data[(size_t)y * wide + x] = (uint8_t)(x * 3 + y * 13 + channel);
    }
    const uint8_t* read = heif_image_get_plane_readonly(image, channel, &stride);
    const uint8_t* read2 = heif_image_get_plane_readonly2(image, channel, &wide);
    uint64_t hash = 14695981039346656037ULL;
    for (size_t i = 0; i < wide * (size_t)h; ++i) hash = (hash ^ read[i]) * 1099511628211ULL;
    printf(":same%d:%llu", read == data && read2 == data, (unsigned long long)hash);
  }
  printf(":nullstride%d%d%d%d", heif_image_get_plane(image, channel, NULL) == NULL,
         heif_image_get_plane_readonly(image, channel, NULL) == NULL,
         heif_image_get_plane2(image, channel, NULL) == NULL,
         heif_image_get_plane_readonly2(image, channel, NULL) == NULL);
}

int main(void) {
  err(heif_error_success); /* Validate the exported data object, not just functions. */
  err(heif_image_create(1, 1, heif_colorspace_RGB, heif_chroma_444, NULL));
  heif_image_release(NULL);
  heif_image_set_premultiplied_alpha(NULL, 1);
  printf(" nullalpha%d\n", heif_image_is_premultiplied_alpha(NULL));
  int colors[] = {-1, 0, 1, 2, 3, 4, 99};
  int chromas[] = {-1, 0, 1, 2, 3, 10, 11, 12, 13, 14, 15, 99};
  int sizes[][2] = {{0,0}, {-1,-1}, {1,1}, {7,5}, {63,65}, {129,3}};
  int depths[] = {0, 1, 7, 8, 9, 10, 12, 16, 24, 32, 64, 128, 129};
  for (unsigned c = 0; c < sizeof(colors)/sizeof(colors[0]); ++c)
    for (unsigned h = 0; h < sizeof(chromas)/sizeof(chromas[0]); ++h)
      for (unsigned s = 0; s < sizeof(sizes)/sizeof(sizes[0]); ++s)
        for (unsigned d = 0; d < sizeof(depths)/sizeof(depths[0]); ++d) {
          heif_image* image = (heif_image*)(uintptr_t)0x1234;
          struct heif_error e = heif_image_create(sizes[s][0], sizes[s][1], colors[c], chromas[h], &image);
          printf("%u/%u/%u/%u", c,h,s,d); err(e);
          printf(" null=%d", image == NULL);
          if (!e.code) {
            uint32_t a = 0, b = 0;
            heif_image_get_pixel_aspect_ratio(image, &a, &b);
            printf(" %d/%d/%d/%d aspect%u/%u", heif_image_get_primary_width(image),
                   heif_image_get_primary_height(image), heif_image_get_colorspace(image),
                   heif_image_get_chroma_format(image), a,b);
            for (int alpha = -1; alpha < 2; ++alpha) {
              heif_image_set_premultiplied_alpha(image, alpha);
              printf(" alpha%d", heif_image_is_premultiplied_alpha(image));
            }
            heif_image_set_pixel_aspect_ratio(image, 0xffffffffu, 0);
            heif_image_get_pixel_aspect_ratio(image, &a, &b);
            printf(" aspect%u/%u", a,b);
            int channel = chromas[h] >= 10 && chromas[h] <= 15 ? heif_channel_interleaved : heif_channel_Y;
            inspect(image, channel, 0);
            // Negative image dimensions are valid metadata to create, but do
            // not request a giant allocation through the legacy unbounded API.
            if (sizes[s][0] >= 0 && sizes[s][1] >= 0) {
              e = heif_image_add_plane(image, channel, sizes[s][0], sizes[s][1], depths[d]); err(e);
              inspect(image, channel, 1);
              if (!e.code) {
                err(heif_image_add_plane(image, channel, 2, 2, depths[d]));
                inspect(image, channel, 0); /* Duplicate channel preserves first. */
              }
            }
            heif_image_release(image);
          }
          putchar('\n');
        }
  return 0;
}
