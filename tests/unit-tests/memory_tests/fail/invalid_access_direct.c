/*
 * Access an invalid (unmapped) address directly.
 * Exercises lind-wasm's PROT_NONE linear memory model: pages not explicitly
 * mapped by rawposix vmmap are inaccessible, and the access should trigger a
 * wasm trap (on wasm) or SIGSEGV (on native).
 */

#include <stdio.h>

int main(void) {
    /*
     * 1 GiB. This sits comfortably above everything rawposix maps for this cage
     * -- the stack, the module data region, the grate worker stack arena (which
     * scales with the worker count and per-worker stack size), and the (tiny)
     * heap -- yet remains well within the 4 GiB wasm linear memory.  The page is
     * therefore reserved PROT_NONE and the access faults instead of reading
     * mapped memory.  (A lower address like 0x1234567 can fall inside the grate
     * stack arena and read as valid.)
     */
    volatile int *addr = (volatile int *)0x40000000;
    int val = *addr;   /* expected to trap / fault */
    printf("val=%d\n", val);
    return 0;
}
