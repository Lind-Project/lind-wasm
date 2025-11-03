#ifndef __x86_64__
#  include <sys/platform/x86.h>

#  define IS_SUPPORTED() CPU_FEATURE_ACTIVE (SSE2)
#endif

/* Clear XMM0...XMM7  */
#define PREPARE_MALLOC() {
}

#include <elf/tst-gnu2-tls2.c>
