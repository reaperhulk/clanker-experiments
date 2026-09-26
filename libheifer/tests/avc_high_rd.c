/* SPDX-License-Identifier: LGPL-3.0-or-later */
// Encodes planar YCbCr (or monochrome) samples through libheif's AVC encoder,
// for tools/bench_avc_high.py.
// usage: avc_high_rd W H QUALITY CHROMA DEPTH in.yuv out.heif ENCODER-ID
//   CHROMA 400, 420, 422 or 444; samples 8-bit, or 16-bit little-endian above 8 bits.
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void chk(struct heif_error e) {
  if (e.code) {
    fprintf(stderr, "err %d %d %s\n", e.code, e.subcode, e.message);
    exit(1);
  }
}

int main(int argc, char** argv) {
  if (argc != 9) return 2;
  int w = atoi(argv[1]), h = atoi(argv[2]), q = atoi(argv[3]), chroma = atoi(argv[4]), depth = atoi(argv[5]);
  int bps = depth > 8 ? 2 : 1;
  int sx = (chroma == 420 || chroma == 422) ? 2 : 1, sy = chroma == 420 ? 2 : 1;
  int cw = (w + sx - 1) / sx, ch = (h + sy - 1) / sy;
  size_t total = (size_t)w * h * bps + (chroma == 400 ? 0 : 2 * (size_t)cw * ch * bps);
  unsigned char* raw = malloc(total);
  FILE* f = fopen(argv[6], "rb");
  if (!f || fread(raw, 1, total, f) != total) return 3;
  fclose(f);
  enum heif_chroma format = chroma == 400 ? heif_chroma_monochrome
                          : chroma == 420 ? heif_chroma_420
                          : chroma == 422 ? heif_chroma_422 : heif_chroma_444;
  struct heif_image* img;
  chk(heif_image_create(w, h, chroma == 400 ? heif_colorspace_monochrome : heif_colorspace_YCbCr, format, &img));
  const enum heif_channel channels[3] = {heif_channel_Y, heif_channel_Cb, heif_channel_Cr};
  unsigned char* src = raw;
  for (int c = 0; c < (chroma == 400 ? 1 : 3); c++) {
    int pw = c ? cw : w, ph = c ? ch : h;
    chk(heif_image_add_plane(img, channels[c], pw, ph, depth));
    int stride;
    uint8_t* p = heif_image_get_plane(img, channels[c], &stride);
    for (int y = 0; y < ph; y++) {
      if (bps == 1) {
        memcpy(p + (size_t)y * stride, src + (size_t)y * pw, pw);
      } else {
        for (int x = 0; x < pw; x++) {
          const unsigned char* s = src + ((size_t)y * pw + x) * 2;
          uint16_t v = (uint16_t)(s[0] | s[1] << 8);
          memcpy(p + (size_t)y * stride + 2 * x, &v, 2);
        }
      }
    }
    src += (size_t)pw * ph * bps;
  }
  struct heif_context* ctx = heif_context_alloc();
  const struct heif_encoder_descriptor* d[8];
  int n = heif_get_encoder_descriptors(heif_compression_AVC, argv[8], d, 8);
  if (n < 1) {
    fprintf(stderr, "no encoder %s\n", argv[8]);
    return 4;
  }
  struct heif_encoder* enc;
  chk(heif_context_get_encoder(ctx, d[0], &enc));
  chk(heif_encoder_set_lossy_quality(enc, q));
  if (chroma != 400) chk(heif_encoder_set_parameter_string(enc, "chroma", chroma == 420 ? "420" : chroma == 422 ? "422" : "444"));
  struct heif_image_handle* hd;
  chk(heif_context_encode_image(ctx, img, enc, NULL, &hd));
  chk(heif_context_write_to_file(ctx, argv[7]));
  return 0;
}
