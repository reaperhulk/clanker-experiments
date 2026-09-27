/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* The SAME C client is compiled against pinned upstream headers, then linked
 * separately with the reference and the candidate. No candidate headers. */
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void hex(const char* text) {
  while (*text) printf("%02x", (unsigned char)*text++);
}
static void error(struct heif_error e) {
  printf(" %d:%d:", e.code, e.subcode);
  if (e.message) hex(e.message); else printf("NULL");
}
int main(void) {
  printf("%s %u %d %d %d\n", heif_get_version(), heif_get_version_number(),
         heif_get_version_number_major(), heif_get_version_number_minor(),
         heif_get_version_number_maintenance());
  uint32_t length;
  while (fread(&length, sizeof(length), 1, stdin) == 1) {
    if (length > 1024 * 1024) return 2;
    uint8_t* allocation = malloc((size_t)length + 1);
    if (!allocation) return 3;
    uint8_t* data = allocation + 1; /* Deliberately unaligned input. */
    if (fread(data, 1, length, stdin) != length) return 4;
    printf("%u %u %d %d %d ", heif_read_main_brand(data, (int)length),
           heif_read_minor_version_brand(data, (int)length),
           heif_main_brand(data, (int)length), heif_check_filetype(data, (int)length),
           heif_check_jpeg_filetype(data, (int)length));
    hex(heif_get_file_mime_type(data, (int)length));
    const char* brands[] = {"avif", "heic", "mif1", "zzzz", "", "a", "ab", "abc", "\xff\x80\xfe\x81"};
    for (unsigned j = 0; j < sizeof(brands)/sizeof(brands[0]); ++j) {
      uint32_t brand = heif_fourcc_to_brand(brands[j]);
      unsigned char out[6] = {0x5a, 0, 0, 0, 0, 0xa5};
      heif_brand_to_fourcc(brand, (char*)out + 1);
      printf(" %u:%02x%02x%02x%02x%02x%02x:%d", brand,
             out[0], out[1], out[2], out[3], out[4], out[5],
             heif_has_compatible_brand(data, (int)length, brands[j]));
    }
    heif_brand2* list = (heif_brand2*)(uintptr_t)0x1234;
    int count = -1234;
    struct heif_error e = heif_list_compatible_brands(data, (int)length, &list, &count);
    error(e);
    printf(" count=%d unchanged=%d", count, list == (heif_brand2*)(uintptr_t)0x1234);
    if (e.code == 0) {
      for (int i = 0; i < count; ++i) printf(" %u", list[i]);
      heif_free_list_of_compatible_brands(list);
    }
    error(heif_has_compatible_filetype(data, (int)length));
    error(heif_list_compatible_brands(NULL, (int)length, &list, &count));
    error(heif_list_compatible_brands(data, (int)length, NULL, &count));
    error(heif_list_compatible_brands(data, (int)length, &list, NULL));
    error(heif_list_compatible_brands(data, -1, &list, &count));
    error(heif_has_compatible_filetype(NULL, (int)length));
    heif_free_list_of_compatible_brands(NULL);
    heif_brand_to_fourcc(0, NULL);
    printf(" %u %d %d\n", heif_fourcc_to_brand(NULL),
           heif_has_compatible_brand(data, (int)length, NULL),
           heif_check_jpeg_filetype(NULL, (int)length));
    free(allocation);
  }
  return ferror(stdin) ? 5 : 0;
}
