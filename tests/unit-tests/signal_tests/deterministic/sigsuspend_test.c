#include <stdio.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>

static volatile sig_atomic_t got_signal = 0;

static void handler(int sig) {
    (void)sig;
    got_signal = 1;
}

int main(void) {
    struct sigaction sa;
    sigset_t block_mask;
    sigset_t suspend_mask;

    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;

    if (sigaction(SIGALRM, &sa, NULL) == -1) {
        perror("sigaction");
        return 1;
    }

    /*
     * Block SIGALRM first so it cannot be delivered
     * before we call sigsuspend().
     */
    sigemptyset(&block_mask);
    sigaddset(&block_mask, SIGALRM);

    if (sigprocmask(SIG_BLOCK, &block_mask, NULL) == -1) {
        perror("sigprocmask");
        return 1;
    }

    /*
     * During sigsuspend(), use a mask that does NOT block SIGALRM.
     * This lets SIGALRM wake sigsuspend().
     */
    sigemptyset(&suspend_mask);

    alarm(1);

    printf("Waiting in sigsuspend()...\n");

    int ret = sigsuspend(&suspend_mask);

    if (ret == -1 && errno == EINTR && got_signal) {
        printf("sigsuspend() was interrupted by SIGALRM: test passed\n");
        return 0;
    }

    printf("test failed: sigsuspend() returned %d, errno=%d, got_signal=%d\n",
           ret, errno, got_signal);

    return 1;
}