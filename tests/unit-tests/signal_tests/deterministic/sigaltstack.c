#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <string.h>
#include <unistd.h>
#include <assert.h>

#define ALT_STACK_SIZE (SIGSTKSZ)

static volatile int handler_ran = 0;

static void sigusr1_handler(int sig) {
    (void)sig;
    handler_ran = 1;
}

int main(void) {
    void *stack_mem = malloc(ALT_STACK_SIZE);
    assert(stack_mem != NULL);

    /* set alternate signal stack */
    stack_t ss;
    memset(&ss, 0, sizeof(ss));
    ss.ss_sp = stack_mem;
    ss.ss_size = ALT_STACK_SIZE;
    ss.ss_flags = 0;
    assert(sigaltstack(&ss, NULL) == 0);

    /* query current alternate stack */
    stack_t old;
    memset(&old, 0, sizeof(old));
    assert(sigaltstack(NULL, &old) == 0);

    /* sysconf(_SC_MINSIGSTKSZ) must return non-zero */
    long minsz = sysconf(_SC_MINSIGSTKSZ);
    assert(minsz > 0);

    /* install a signal handler with SA_ONSTACK and fire it,
       to make sure sigaltstack + signal delivery doesn't crash.
       NOTE: the stub sigaltstack doesn't actually switch stacks,
       the handler runs on the main stack. this just tests that
       the combination doesn't blow up. */
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = sigusr1_handler;
    sa.sa_flags = SA_ONSTACK;
    assert(sigaction(SIGUSR1, &sa, NULL) == 0);
    assert(raise(SIGUSR1) == 0);
    assert(handler_ran == 1);

    /* disable alternate stack */
    stack_t disable;
    memset(&disable, 0, sizeof(disable));
    disable.ss_flags = SS_DISABLE;
    assert(sigaltstack(&disable, NULL) == 0);

    free(stack_mem);
    printf("all sigaltstack tests passed\n");
    return 0;
}
