/*
 * Access an invalid (unmapped) address directly.
 * Exercises lind-wasm's PROT_NONE linear memory model: pages not explicitly
 * mapped by rawposix vmmap are inaccessible, and the access should trigger a
 * wasm trap (on wasm) or SIGSEGV (on native).
 */

#include <stdio.h>

int main(void) {
    volatile int *addr = (volatile int *)0x1234567;
    int val = *addr;   /* expected to trap / fault */
    printf("val=%d\n", val);
    return 0;
}
