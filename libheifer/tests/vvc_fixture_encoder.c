/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Test-only native VVC fixture encoder driving the pinned vvenc library the way
 * libheif's vvenc plugin does (vvenc_init_default, explicit bit depths, 4:0:0 via
 * m_internChromaFormat). vvenc is never a candidate dependency; this only writes
 * Annex B streams for tools/generate_vvc_fixtures.py.
 *
 * usage: vvc_fixture_encoder WIDTH HEIGHT CHROMA DEPTH PRESET QP INPUT OUTPUT [NAME=VALUE...]
 *   CHROMA  400, 420, 422 or 444
 *   PRESET  faster, fast, medium, slow or slower
 *   INPUT   planar 16-bit little-endian samples (Y, then Cb and Cr of
 *           vvenc_get_width_of_component x vvenc_get_height_of_component),
 *           one or more pictures; each becomes one input frame
 *   NAME=VALUE pairs go to vvenc_set_param (vvencapp option names). */
#include <vvenc/vvenc.h>
#include <vvenc/vvencCfg.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void log_message(void* ctx, int level, const char* fmt, va_list args) {
  (void)ctx;
  if (level <= VVENC_WARNING) vfprintf(stderr, fmt, args);
}

static int fail(const char* what, const char* detail) {
  fprintf(stderr, "%s%s%s\n", what, detail ? ": " : "", detail ? detail : "");
  return 2;
}

static int write_au(FILE* out, const vvencAccessUnit* au) {
  return au->payloadUsedSize > 0 && fwrite(au->payload, 1, (size_t)au->payloadUsedSize, out) != (size_t)au->payloadUsedSize;
}

int main(int argc, char** argv) {
  if (argc < 9) return fail("usage", "WIDTH HEIGHT CHROMA DEPTH PRESET QP INPUT OUTPUT [NAME=VALUE...]");
  int width = atoi(argv[1]), height = atoi(argv[2]), chroma = atoi(argv[3]), depth = atoi(argv[4]), qp = atoi(argv[6]);
  vvencChromaFormat format = chroma == 400 ? VVENC_CHROMA_400 : chroma == 420 ? VVENC_CHROMA_420
                           : chroma == 422 ? VVENC_CHROMA_422 : chroma == 444 ? VVENC_CHROMA_444 : VVENC_NUM_CHROMA_FORMAT;
  if (format == VVENC_NUM_CHROMA_FORMAT) return fail("bad chroma", argv[3]);
  static const char* names[] = {"faster", "fast", "medium", "slow", "slower"};
  int preset = -1;
  for (int i = 0; i < 5; i++)
    if (!strcmp(argv[5], names[i])) preset = i;
  if (preset < 0) return fail("bad preset", argv[5]);

  vvenc_config params;
  if (vvenc_init_default(&params, width, height, 1, 0, qp, (vvencPresetMode)preset) != VVENC_OK)
    return fail("vvenc_init_default", NULL);
  vvenc_set_msg_callback(&params, NULL, log_message);
  params.m_verbosity = VVENC_WARNING;
  params.m_inputBitDepth[0] = params.m_inputBitDepth[1] = depth;
  params.m_outputBitDepth[0] = params.m_outputBitDepth[1] = depth;
  params.m_internalBitDepth[0] = params.m_internalBitDepth[1] = depth;
  params.m_internChromaFormat = format;
  /* libheif's still-image call: frame rate and scale 1/1. */
  params.m_FrameRate = 1;
  params.m_FrameScale = 1;
  /* One frame per picture in INPUT. */
  long frame_bytes = 0;
  for (int c = 0; c < (format == VVENC_CHROMA_400 ? 1 : 3); c++)
    frame_bytes += 2L * vvenc_get_width_of_component(format, width, c) * vvenc_get_height_of_component(format, height, c);
  FILE* probe = fopen(argv[7], "rb");
  if (!probe) return fail("cannot open input", argv[7]);
  fseek(probe, 0, SEEK_END);
  long input_bytes = ftell(probe);
  fclose(probe);
  if (input_bytes <= 0 || input_bytes % frame_bytes) return fail("input is not a whole number of pictures", argv[7]);
  params.m_framesToBeEncoded = (int)(input_bytes / frame_bytes);
  /* Single-threaded for reproducible output. */
  params.m_numThreads = 0;
  params.m_maxParallelFrames = 0;
  for (int i = 9; i < argc; i++) {
    char* eq = strchr(argv[i], '=');
    if (!eq) return fail("option without value", argv[i]);
    *eq = 0;
    int ret = vvenc_set_param(&params, argv[i], eq + 1);
    *eq = '=';
    if (ret != 0) return fail("vvenc_set_param rejected", argv[i]);
  }

  vvencEncoder* encoder = vvenc_encoder_create();
  if (!encoder) return fail("vvenc_encoder_create", NULL);
  if (vvenc_encoder_open(encoder, &params) != VVENC_OK) {
    int status = fail("vvenc_encoder_open", vvenc_get_last_error(encoder));
    vvenc_encoder_close(encoder);
    return status;
  }

  vvencYUVBuffer* yuv = vvenc_YUVBuffer_alloc();
  vvenc_YUVBuffer_alloc_buffer(yuv, format, width, height);
  FILE* in = fopen(argv[7], "rb");
  if (!in) return fail("cannot open input", argv[7]);
  int planes = format == VVENC_CHROMA_400 ? 1 : 3;
  vvencAccessUnit* au = vvenc_accessUnit_alloc();
  vvenc_accessUnit_alloc_payload(au, 3 * width * height + 1024 * 1024);
  FILE* out = fopen(argv[8], "wb");
  if (!out) return fail("cannot open output", argv[8]);
  bool done = false;
  int ret;
  for (int frame = 0;; frame++) {
    int first = fgetc(in);
    if (first == EOF) {
      if (frame == 0) return fail("short input", argv[7]);
      break;
    }
    ungetc(first, in);
    for (int c = 0; c < planes; c++) {
      int pw = vvenc_get_width_of_component(format, width, c), ph = vvenc_get_height_of_component(format, height, c);
      for (int y = 0; y < ph; y++)
        for (int x = 0; x < pw; x++) {
          int lo = fgetc(in), hi = fgetc(in);
          if (lo < 0 || hi < 0) return fail("short input", argv[7]);
          yuv->planes[c].ptr[y * yuv->planes[c].stride + x] = (int16_t)(lo | (hi << 8));
        }
    }
    yuv->cts = frame;
    yuv->ctsValid = true;
    vvenc_accessUnit_reset(au);
    ret = vvenc_encode(encoder, yuv, au, &done);
    if (ret != VVENC_OK) return fail("vvenc_encode", vvenc_get_last_error(encoder));
    if (write_au(out, au)) return fail("write failed", argv[8]);
  }
  fclose(in);
  while (!done) {
    vvenc_accessUnit_reset(au);
    ret = vvenc_encode(encoder, NULL, au, &done);
    if (ret != VVENC_OK) return fail("vvenc_encode flush", vvenc_get_last_error(encoder));
    if (write_au(out, au)) return fail("write failed", argv[8]);
  }
  if (fclose(out)) return fail("close failed", argv[8]);
  vvenc_YUVBuffer_free(yuv, true);
  vvenc_accessUnit_free(au, true);
  vvenc_encoder_close(encoder);
  return 0;
}
