/* Test: single-threaded program that returns from main (no explicit exit()).
 * glibc's __libc_start_call_main should call exit(result) after main returns.
 * If this hangs, the basic exit syscall path is broken.
 */
#include <stdio.h>

int main(void) {
    printf("exiting via return\n");
    return 0;
}
