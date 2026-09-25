/* Prints the default encoder of a compression format (argv[1]): lossy/lossless
   support and every parameter with its type, default flag, valid values and
   current value. Names and ids are not printed (they name the implementation). */
#include <stdio.h>
#include <stdlib.h>
#include <libheif/heif.h>

static void error(heif_error e) { printf(" e%d,%d,%s", e.code, e.subcode, e.message ? e.message : "NULL"); }

int main(int argc, char** argv) {
  heif_init(NULL);
  heif_compression_format format = (heif_compression_format)(argc > 1 ? atoi(argv[1]) : 1);
  const heif_encoder_descriptor* d[16];
  int n = heif_get_encoder_descriptors(format, NULL, d, 16);
  printf("descriptors%d", n);
  if (n < 1) { printf("\n"); return 0; }
  printf(" lossy%d lossless%d", heif_encoder_descriptor_supports_lossy_compression(d[0]),
         heif_encoder_descriptor_supports_lossless_compression(d[0]));
  heif_encoder* enc = NULL;
  error(heif_context_get_encoder(NULL, d[0], &enc));
  if (!enc) { printf("\n"); return 0; }
  printf("\n");
  for (const heif_encoder_parameter* const* p = heif_encoder_list_parameters(enc); *p; p++) {
    const char* name = heif_encoder_parameter_get_name(*p);
    int type = heif_encoder_parameter_get_type(*p);
    printf("%s type%d default%d", name, type, heif_encoder_has_default(enc, name));
    if (type == heif_encoder_parameter_type_integer) {
      int have_min = -1, have_max = -1, min = -1, max = -1, count = -1, value = -1;
      const int* values = NULL;
      error(heif_encoder_parameter_get_valid_integer_values(*p, &have_min, &have_max, &min, &max, &count, &values));
      printf(" range%d,%d,%d,%d values%d", have_min, have_max, min, max, count);
      for (int i = 0; values && i < count; i++) printf(",%d", values[i]);
      error(heif_encoder_get_parameter_integer(enc, name, &value));
      printf(" value%d", value);
    } else if (type == heif_encoder_parameter_type_boolean) {
      int value = -1;
      error(heif_encoder_get_parameter_boolean(enc, name, &value));
      printf(" value%d", value);
    } else if (type == heif_encoder_parameter_type_string) {
      const char* const* strings = NULL;
      error(heif_encoder_parameter_get_valid_string_values(*p, &strings));
      printf(" strings");
      for (int i = 0; strings && strings[i]; i++) printf(",%s", strings[i]);
      char value[256] = "";
      error(heif_encoder_get_parameter_string(enc, name, value, sizeof(value)));
      printf(" value%s", value);
    }
    char generic[256] = "";
    error(heif_encoder_get_parameter(enc, name, generic, sizeof(generic)));
    printf(" generic%s\n", generic);
  }
  heif_encoder_release(enc);
  heif_deinit();
  return 0;
}
