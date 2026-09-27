/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Test-only native HEVC fixture encoder driving the pinned x265 library (the
 * reference build's multilib libx265: 8, 10 and 12 bit). x265 is never a
 * candidate dependency; this only writes one-picture Annex B streams for
 * tools/generate_hevc_rext_fixtures.py.
 *
 * usage: hevc_fixture_encoder WIDTH HEIGHT CHROMA DEPTH PRESET INPUT OUTPUT [NAME=VALUE...]
 *   CHROMA  400, 420, 422 or 444
 *   INPUT   planar samples (Y, then Cb and Cr), 8-bit for depth 8 and 16-bit
 *           little-endian otherwise
 *   NAME=VALUE pairs go to x265_param_parse (x265 CLI option names). */
#include <x265.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int fail(const char* what, const char* detail) {
  fprintf(stderr, "%s%s%s\n", what, detail ? ": " : "", detail ? detail : "");
  return 2;
}

static int write_nals(FILE* out, x265_nal* nals, uint32_t count) {
  for (uint32_t i = 0; i < count; i++)
    if (fwrite(nals[i].payload, 1, nals[i].sizeBytes, out) != nals[i].sizeBytes) return 1;
  return 0;
}

int main(int argc, char** argv) {
  if (argc < 8) return fail("usage", "WIDTH HEIGHT CHROMA DEPTH PRESET INPUT OUTPUT [NAME=VALUE...]");
  int width = atoi(argv[1]), height = atoi(argv[2]), chroma = atoi(argv[3]), depth = atoi(argv[4]);
  int csp = chroma == 400 ? X265_CSP_I400 : chroma == 420 ? X265_CSP_I420
          : chroma == 422 ? X265_CSP_I422 : chroma == 444 ? X265_CSP_I444 : -1;
  if (csp < 0) return fail("bad chroma", argv[3]);
  const x265_api* api = x265_api_get(depth);
  if (!api) return fail("no x265 library for depth", argv[4]);
  x265_param* param = api->param_alloc();
  if (api->param_default_preset(param, argv[5], NULL) < 0) return fail("bad preset", argv[5]);
  param->sourceWidth = width;
  param->sourceHeight = height;
  param->internalCsp = csp;
  param->internalBitDepth = depth;
  param->sourceBitDepth = depth;
  param->fpsNum = 1;
  param->fpsDenom = 1;
  param->totalFrames = 1;
  param->bRepeatHeaders = 1;
  param->logLevel = X265_LOG_ERROR;
  static const char* defaults[][2] = {{"keyint", "1"}, {"info", "0"}, {"frame-threads", "1"}, {"pools", "none"}};
  for (size_t i = 0; i < sizeof defaults / sizeof *defaults; i++)
    if (api->param_parse(param, defaults[i][0], defaults[i][1])) return fail("bad default", defaults[i][0]);
  /* As libheif's x265 plugin: the largest CTU (64, 32, 16) that fits the picture. */
  int ctu = 64;
  while (ctu > 16 && (width < ctu || height < ctu)) ctu /= 2;
  char ctu_value[4];
  snprintf(ctu_value, sizeof ctu_value, "%d", ctu);
  if (api->param_parse(param, "ctu", ctu_value)) return fail("bad default", "ctu");
  for (int i = 8; i < argc; i++) {
    char* eq = strchr(argv[i], '=');
    if (!eq) return fail("bad option", argv[i]);
    *eq = 0;
    if (api->param_parse(param, argv[i], eq + 1)) return fail("bad option", argv[i]);
  }

  int sub_x = chroma == 420 || chroma == 422, sub_y = chroma == 420;
  int bytes = depth > 8 ? 2 : 1, planes = chroma == 400 ? 1 : 3;
  size_t sizes[3], total = 0;
  for (int p = 0; p < planes; p++) {
    int w = p ? (width + sub_x) >> sub_x : width, h = p ? (height + sub_y) >> sub_y : height;
    sizes[p] = (size_t)w * h * bytes;
    total += sizes[p];
  }
  FILE* in = fopen(argv[6], "rb");
  if (!in) return fail("cannot open input", argv[6]);
  unsigned char* raw = malloc(total);
  if (!raw || fread(raw, 1, total, in) != total) return fail("short input", argv[6]);
  fclose(in);

  x265_encoder* encoder = api->encoder_open(param);
  if (!encoder) return fail("x265 encoder could not be opened", NULL);
  x265_picture* picture = api->picture_alloc();
  api->picture_init(param, picture);
  unsigned char* plane = raw;
  for (int p = 0; p < planes; p++) {
    int w = p ? (width + sub_x) >> sub_x : width;
    picture->planes[p] = plane;
    picture->stride[p] = w * bytes;
    plane += sizes[p];
  }
  picture->bitDepth = depth;
  FILE* out = fopen(argv[7], "wb");
  if (!out) return fail("cannot open output", argv[7]);
  x265_nal* nals;
  uint32_t count;
  int result = api->encoder_encode(encoder, &nals, &count, picture, NULL);
  if (result < 0) return fail("encode failed", NULL);
  if (result > 0 && write_nals(out, nals, count)) return fail("write failed", argv[7]);
  while ((result = api->encoder_encode(encoder, &nals, &count, NULL, NULL)) > 0)
    if (write_nals(out, nals, count)) return fail("write failed", argv[7]);
  if (result < 0) return fail("flush failed", NULL);
  fclose(out);
  api->encoder_close(encoder);
  api->picture_free(picture);
  api->param_free(param);
  free(raw);
  return 0;
}
