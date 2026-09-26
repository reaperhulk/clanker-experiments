/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* `cc` for the Windows (MinGW-w64) C-client job: runs gcc with the given
   arguments and, when linking, adds MinGW's binmode.o so stdin/stdout and
   fopen default to binary mode as on POSIX (the clients stream binary
   payloads through stdin). BINMODE is gcc -print-file-name=binmode.o. */
#include <process.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char* quote(const char* s) {
  if (*s && !strpbrk(s, " \t\"")) return (char*)s;
  size_t n = strlen(s);
  char* q = malloc(2 * n + 3);
  char* o = q;
  *o++ = '"';
  for (; *s; s++) {
    if (*s == '"' || *s == '\\') *o++ = '\\';
    *o++ = *s;
  }
  *o++ = '"';
  *o = 0;
  return q;
}

int main(int argc, char** argv) {
  int link = 1;
  for (int i = 1; i < argc; i++) {
    const char* a = argv[i];
    if (!strcmp(a, "-c") || !strcmp(a, "-E") || !strcmp(a, "-S") || !strcmp(a, "-fsyntax-only") || !strncmp(a, "-print-", 7)) link = 0;
  }
  char** args = calloc((size_t)argc + 2, sizeof *args);
  int n = 0;
  args[n++] = "gcc";
  for (int i = 1; i < argc; i++) args[n++] = quote(argv[i]);
  if (link) args[n++] = quote(BINMODE);
  args[n] = NULL;
  fflush(stdout);
  intptr_t r = _spawnvp(_P_WAIT, "gcc", (const char* const*)args);
  return r < 0 ? 127 : (int)r;
}
