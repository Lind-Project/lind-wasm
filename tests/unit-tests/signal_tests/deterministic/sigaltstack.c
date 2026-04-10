#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <string.h>
#include <unistd.h>

#define ALT_STACK_SIZE (SIGSTKSZ)

int main(void) {
    /* Test 1: sigaltstack symbol exists and accepts a stack */
    void *stack_mem = malloc(ALT_STACK_SIZE);
    if (!stack_mem) {
        perror("malloc");
        return 1;
    }

    stack_t ss;
    memset(&ss, 0, sizeof(ss));
    ss.ss_sp = stack_mem;
    ss.ss_size = ALT_STACK_SIZE;
    ss.ss_flags = 0;

    if (sigaltstack(&ss, NULL) != 0) {
        perror("sigaltstack set");
        free(stack_mem);
        return 1;
    }
    printf("sigaltstack set: ok\n");

    /* Test 2: query the current alternate stack */
    stack_t old;
    memset(&old, 0, sizeof(old));
    if (sigaltstack(NULL, &old) != 0) {
        perror("sigaltstack query");
        free(stack_mem);
        return 1;
    }
    printf("sigaltstack query: ok\n");

    /* Test 3: sysconf(_SC_MINSIGSTKSZ) returns non-zero */
    long minsz = sysconf(_SC_MINSIGSTKSZ);
    if (minsz <= 0) {
        fprintf(stderr, "sysconf(_SC_MINSIGSTKSZ) returned %ld, expected > 0\n", minsz);
        free(stack_mem);
        return 1;
    }
    printf("sysconf(_SC_MINSIGSTKSZ): %ld\n", minsz);

    /* Test 4: disable alternate stack */
    stack_t disable;
    memset(&disable, 0, sizeof(disable));
    disable.ss_flags = SS_DISABLE;
    if (sigaltstack(&disable, NULL) != 0) {
        perror("sigaltstack disable");
        free(stack_mem);
        return 1;
    }
    printf("sigaltstack disable: ok\n");

    free(stack_mem);
    printf("all sigaltstack tests passed\n");
    return 0;
}
