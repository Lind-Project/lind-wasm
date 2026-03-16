#include <signal.h>
#include <stdio.h>
#include <unistd.h>

volatile sig_atomic_t got_usr1 = 0;

static void handler(int sig) {
    (void)sig;
    got_usr1 = 1;
}

int main(void) {
    struct sigaction sa;
    sigemptyset(&sa.sa_mask);
    sa.sa_handler = handler;
    sa.sa_flags = 0;
    if (sigaction(SIGUSR1, &sa, NULL) != 0) {
        fprintf(stderr, "sigaction failed\n");
        return 1;
    }

    sigset_t block_mask;
    sigemptyset(&block_mask);
    sigaddset(&block_mask, SIGUSR1);
    if (sigprocmask(SIG_BLOCK, &block_mask, NULL) != 0) {
        fprintf(stderr, "sigprocmask block failed\n");
        return 1;
    }

    if (kill(getpid(), SIGUSR1) != 0) {
        fprintf(stderr, "kill failed\n");
        return 1;
    }
    if (got_usr1 != 0) {
        fprintf(stderr, "got_usr1 not 0 while blocked\n");
        return 1;
    }

    sigset_t wait_mask;
    sigemptyset(&wait_mask);
    sigsuspend(&wait_mask);

    if (got_usr1 != 1) {
        fprintf(stderr, "got_usr1 not 1 after delivery\n");
        return 1;
    }
    return 0;
}
