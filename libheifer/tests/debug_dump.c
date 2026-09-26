/* SPDX-License-Identifier: LGPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include <libheif/heif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
static void dump(heif_context* ctx) {
 FILE* f=tmpfile();if(!f)exit(3);int fd=fileno(f);heif_context_debug_dump_boxes_to_file(ctx,fd);if(fputc(0xa5,f)==EOF||fflush(f))exit(4);long n=ftell(f);rewind(f);printf(" dump%ld:",n);int b;while((b=fgetc(f))!=EOF)printf("%02x",b);fclose(f);
}
int main(int argc,char** argv) { if(argc<2)return 2; uint32_t length;while(fread(&length,4,1,stdin)==1){if(length>2000000)return 2;unsigned char* bytes=malloc((size_t)length+1);if(fread(bytes,1,length,stdin)!=length)return 2;for(int mode=0;mode<2;mode++){heif_context* ctx=heif_context_alloc();dump(NULL);dump(ctx);heif_error e;if(mode){FILE* file=fopen(argv[1],"wb");if(!file||fwrite(bytes,1,length,file)!=length)return 3;fclose(file);e=heif_context_read_from_file(ctx,argv[1],NULL);unlink(argv[1]);}else{e=heif_context_read_from_memory_without_copy(ctx,bytes,length,NULL);}printf(" read%d:%d",e.code,e.subcode);dump(ctx);dump(ctx);heif_context_debug_dump_boxes_to_file(ctx,-1);heif_context_read_from_memory(ctx,"x",1,NULL);dump(ctx);heif_context_free(ctx);}free(bytes);puts("");}return ferror(stdin)?2:0;}
