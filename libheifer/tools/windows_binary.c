/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* Linked into every C client by tools/windows_cc.c on Windows: switch the
   standard streams to binary mode before main() and make fopen default to
   binary, as on POSIX (the clients stream binary payloads through stdin). */
#include <fcntl.h>
#include <io.h>
#include <stdlib.h>

__attribute__((constructor)) static void binary_stdio(void) {
  _setmode(0, _O_BINARY);
  _setmode(1, _O_BINARY);
  _setmode(2, _O_BINARY);
  _set_fmode(_O_BINARY);
}
