#include <stdio.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>

static volatile sig_atomic_t got_signal = 0;

static void handler(int sig) {
    (void)sig;
    got_signal = 1;
}

/*
 * Test 1: signal arrives while sigsuspend is blocking (alarm fires after entry).
 * Verifies the basic sigsuspend/signal-delivery path.
 */
static int test_signal_during_wait(void) {
    struct sigaction sa;
    sigset_t block_mask, suspend_mask;

    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;

    if (sigaction(SIGALRM, &sa, NULL) == -1) {
        perror("sigaction");
        return 1;
    }

    sigemptyset(&block_mask);
    sigaddset(&block_mask, SIGALRM);
    if (sigprocmask(SIG_BLOCK, &block_mask, NULL) == -1) {
        perror("sigprocmask");
        return 1;
    }

    sigemptyset(&suspend_mask);
    got_signal = 0;
    alarm(1);

    printf("  [1] Waiting in sigsuspend() for alarm...\n");
    int ret = sigsuspend(&suspend_mask);

    sigprocmask(SIG_UNBLOCK, &block_mask, NULL);
    alarm(0);

    if (ret == -1 && errno == EINTR && got_signal) {
        printf("  [1] PASS: sigsuspend() interrupted by SIGALRM\n");
        return 0;
    }
    printf("  [1] FAIL: ret=%d errno=%d got_signal=%d\n", ret, errno, got_signal);
    return 1;
}

/*
 * Test 2: signal is already pending before sigsuspend is called.
 *
 * Block SIGUSR1, kill(getpid(), SIGUSR1) to make it pending, then call
 * sigsuspend() with a mask that unblocks it.  The syscall must deliver the
 * signal atomically (mask swap + wait in one host call) and return EINTR
 * immediately rather than hanging.
 *
 * A watchdog alarm catches the hang that the old sigprocmask+pause
 * implementation produced: sigprocmask triggered the epoch, signal_callback
 * fired at pause's function-header injection point (consuming the epoch),
 * and pause_syscall then looped forever seeing EPOCH_NORMAL.
 */
static int test_pending_signal_before_call(void) {
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

    /* SIGUSR1 is now pending (blocked, so epoch is NOT triggered yet) */
    got_signal = 0;
    kill(getpid(), SIGUSR1);

    /* Watchdog: if sigsuspend hangs, SIGALRM default action kills the process */
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

    printf("=== sigsuspend tests ===\n");
    failed += test_signal_during_wait();
    failed += test_pending_signal_before_call();

    if (failed == 0)
        printf("All tests passed\n");
    else
        printf("%d test(s) failed\n", failed);

    return failed > 0 ? 1 : 0;
}
