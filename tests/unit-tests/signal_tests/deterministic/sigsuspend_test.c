/*
 * Test sigsuspend(2):
 *   1. Atomically unblocks signals and waits — a signal blocked before the
 *      call is delivered immediately when sigsuspend opens the mask.
 *   2. Returns -1/EINTR after the handler runs.
 *   3. Restores the original signal mask on return.
 */

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <unistd.h>

static volatile sig_atomic_t handler_ran = 0;

static void handler(int sig) {
    (void)sig;
    handler_ran = 1;
}

int main(void) {
    struct sigaction sa = {0};
    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    if (sigaction(SIGUSR1, &sa, NULL) != 0) {
        perror("sigaction");
        return 1;
    }

    /* Block SIGUSR1 so the kill() below queues it without firing. */
    sigset_t block, empty;
    sigemptyset(&block);
    sigaddset(&block, SIGUSR1);
    if (sigprocmask(SIG_BLOCK, &block, NULL) != 0) {
        perror("sigprocmask block");
        return 1;
    }

    kill(getpid(), SIGUSR1);   /* pending, not yet delivered */

    /* sigsuspend with empty mask: atomically unblocks all signals. */
    sigemptyset(&empty);
    int ret = sigsuspend(&empty);

    /* 1. Must return -1/EINTR */
    if (ret != -1 || errno != EINTR) {
        fprintf(stderr, "FAIL: sigsuspend returned %d errno %d, expected -1/EINTR\n",
                ret, errno);
        return 1;
    }

    /* 2. Handler must have run */
    if (!handler_ran) {
        fprintf(stderr, "FAIL: signal handler did not run\n");
        return 1;
    }

    /* 3. Original mask must be restored (SIGUSR1 blocked again) */
    sigset_t cur;
    sigemptyset(&cur);
    sigprocmask(SIG_BLOCK, &empty, &cur);   /* query: block nothing, get current */
    if (!sigismember(&cur, SIGUSR1)) {
        fprintf(stderr, "FAIL: signal mask not restored after sigsuspend\n");
        return 1;
    }

    printf("PASS\n");
    return 0;
}
