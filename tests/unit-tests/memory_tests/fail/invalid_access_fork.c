/*
 * Access an invalid (unmapped) address inside a forked child.
 * Verifies that each cage created by fork() has its own fresh PROT_NONE
 * address space: accesses to unmapped pages in the child trigger a wasm trap
 * (on wasm) or SIGSEGV (on native).  The parent waits for the child and exits
 * non-zero when the child crashes.
 */

#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    pid_t pid = fork();
    if (pid < 0) {
        return 1;
    }

    if (pid == 0) {
        /*
         * child: dereference an unmapped address.  1 GiB sits above everything
         * rawposix maps for this cage -- stack, data region, grate worker stack
         * arena (scales with worker count and per-worker stack size), and the
         * tiny heap -- yet stays within the 4 GiB wasm linear memory, so the
         * page is reserved PROT_NONE and the access faults.  (A lower address
         * like 0x1234567 can fall inside the grate stack arena and read as valid.)
         */
        volatile int *addr = (volatile int *)0x40000000;
        int val = *addr;   /* expected to trap / fault */
        printf("val=%d\n", val);
        exit(0);
    }

    /* parent: wait and propagate child failure */
    int status;
    waitpid(pid, &status, 0);
    if (WIFEXITED(status) && WEXITSTATUS(status) == 0)
        return 0;
    return 1;
}
