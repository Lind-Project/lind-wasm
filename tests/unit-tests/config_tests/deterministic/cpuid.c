#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <stdint.h>
#include <stdio.h>

int main(void) {
#if defined(__wasm__) || defined(__wasi__) || !(defined(__i386__) || defined(__x86_64__))
    /* Wasm/WASI or non-x86 native: no inline cpuid */
    puts("cpuid-ok");
#else
    /* x86/x86_64 native: execute cpuid for EAX=0 just to exercise it */
    unsigned int eax, ebx, ecx, edx;
    eax = 0;
    __asm__ __volatile__(
        "cpuid"
        : "=a"(eax), "=b"(ebx), "=c"(ecx), "=d"(edx)
        : "a"(0)
        : "cc"
    );
    /* We ignore the vendor bytes to keep output identical to Wasm. */
    puts("cpuid-ok");
#endif
    return 0;
}

