#include <stdio.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>
#include <stdlib.h>

static volatile sig_atomic_t got_signal = 0;

static void handler(int sig) {
    (void)sig;
    got_signal = 1;
}

/*
 * Test 1: pause() is interrupted by a signal that arrives while blocking.
 * Verifies the basic pause/signal-delivery path.
 */
static int test_signal_during_wait(void) {
    struct sigaction sa;
    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;

    if (sigaction(SIGALRM, &sa, NULL) == -1) {
        perror("sigaction");
        return 1;
    }

    got_signal = 0;
    alarm(1);

    printf("  [1] Waiting in pause()...\n");
    int ret = pause();
    alarm(0);

    if (ret == -1 && errno == EINTR && got_signal) {
        printf("  [1] PASS: pause() interrupted by SIGALRM\n");
        return 0;
    }
    printf("  [1] FAIL: ret=%d errno=%d got_signal=%d\n", ret, errno, got_signal);
    return 1;
}

/*
 * Test 2: signal is already pending before the wait call.
 *
 * pause() passes the current mask to sigsuspend(), so it cannot atomically
 * unblock a blocked signal — use sigsuspend() with an empty mask for that.
 * Block SIGUSR1, make it pending via kill(), then call sigsuspend() with an
 * empty mask.  This exercises the same rt_sigsuspend syscall path that pause()
 * uses and confirms the pending-signal case works end-to-end.
 *
 * A watchdog alarm detects a hang.
 */
static int test_pending_signal_before_wait(void) {
    struct sigaction sa;
    sigset_t block_mask, empty_mask;

    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;

    if (sigaction(SIGUSR1, &sa, NULL) == -1) {
        perror("sigaction");
        return 1;
    }

    sigemptyset(&block_mask);
    sigaddset(&block_mask, SIGUSR1);
    if (sigprocmask(SIG_BLOCK, &block_mask, NULL) == -1) {
        perror("sigprocmask");
        return 1;
    }

    /* SIGUSR1 is now pending (blocked — epoch not yet triggered) */
    got_signal = 0;
    kill(getpid(), SIGUSR1);

    /* Watchdog: kill process if the call hangs */
    alarm(3);

    sigemptyset(&empty_mask);

    printf("  [2] Calling sigsuspend() with pre-pending SIGUSR1...\n");
    int ret = sigsuspend(&empty_mask);

    alarm(0);
    sigprocmask(SIG_UNBLOCK, &block_mask, NULL);
    signal(SIGUSR1, SIG_DFL);

    if (ret == -1 && errno == EINTR && got_signal) {
        printf("  [2] PASS: sigsuspend() returned immediately, pending signal delivered\n");
        return 0;
    }
    printf("  [2] FAIL: ret=%d errno=%d got_signal=%d\n", ret, errno, got_signal);
    return 1;
}

int main(void) {
    int failed = 0;

    printf("=== pause / sigsuspend pending-signal tests ===\n");
    failed += test_signal_during_wait();
    failed += test_pending_signal_before_wait();

    if (failed == 0)
        printf("All tests passed\n");
    else
        printf("%d test(s) failed\n", failed);

    return failed > 0 ? 1 : 0;
}
