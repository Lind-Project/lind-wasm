#include <setjmp.h>
#include <signal.h>
#include <stdio.h>
#include <unistd.h>

static sigjmp_buf env;
static volatile sig_atomic_t jumped = 0;

static void handler(int sig) {
    (void)sig;
    jumped = 1;
    siglongjmp(env, 1);
}

int main(void) {
    sigset_t block_mask, wait_mask;
    struct sigaction sa;

    sa.sa_handler = handler;
    sa.sa_flags = 0;
    if (sigemptyset(&sa.sa_mask) != 0 ||
        sigaction(SIGUSR1, &sa, NULL) != 0) {
        fprintf(stderr, "signal_longjmp: sigaction failed\n");
        return 1;
    }

    if (sigemptyset(&block_mask) != 0 || sigaddset(&block_mask, SIGUSR1) != 0 ||
        sigprocmask(SIG_BLOCK, &block_mask, NULL) != 0) {
        fprintf(stderr, "signal_longjmp: sigprocmask block failed\n");
        return 1;
    }

    if (sigsetjmp(env, 1) == 0) {
        if (kill(getpid(), SIGUSR1) != 0) {
            fprintf(stderr, "signal_longjmp: kill failed\n");
            return 1;
        }
        sigemptyset(&wait_mask);
        sigsuspend(&wait_mask);
        fprintf(stderr, "signal_longjmp: sigsuspend returned without longjmp\n");
        return 1;
    }

    if (jumped != 1) {
        fprintf(stderr, "signal_longjmp: jumped not set\n");
        return 1;
    }
    return 0;
}
