#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <signal.h>
#include <stdint.h>
#include <sys/wait.h>

void test_sigaction_basic() {
    printf("[TEST 1] Install new handler and retrieve old handler\n");

    struct sigaction new_action;
    struct sigaction old_action;

    new_action.sa_handler = SIG_IGN;
    sigemptyset(&new_action.sa_mask);
    new_action.sa_flags = 0;

    int ret = sigaction_syscall(0, SIGUSR1, 0,
                                (uint64_t)&new_action, 0,
                                (uint64_t)&old_action, 0,
                                0, 0, 0, 0, 0, 0);
    printf("[RET] sigaction_syscall returned=%d\n", ret);
}

void test_sigaction_old_only() {
    printf("\n[TEST 2] Retrieve old handler only\n");

    struct sigaction old_action;
    int ret = sigaction_syscall(0, SIGUSR1, 0,
                                0, 0,
                                (uint64_t)&old_action, 0,
                                0, 0, 0, 0, 0, 0);
    printf("[RET] sigaction_syscall returned=%d\n", ret);
}

void test_sigaction_invalid() {
    printf("\n[TEST 3] Attempt to modify SIGKILL and SIGSTOP\n");

    struct sigaction new_action;
    new_action.sa_handler = SIG_IGN;
    sigemptyset(&new_action.sa_mask);
    new_action.sa_flags = 0;

    int ret = sigaction_syscall(0, SIGKILL, 0,
                                (uint64_t)&new_action, 0,
                                0, 0,
                                0, 0, 0, 0, 0, 0);
    printf("[RET] sigaction_syscall SIGKILL returned=%d\n", ret);

    ret = sigaction_syscall(0, SIGSTOP, 0,
                             (uint64_t)&new_action, 0,
                             0, 0,
                             0, 0, 0, 0, 0, 0);
    printf("[RET] sigaction_syscall SIGSTOP returned=%d\n", ret);
}

void test_sigaction_all_signals() {
    printf("\n[TEST 4] Install handler for multiple signals\n");

    struct sigaction new_action, old_action;
    new_action.sa_handler = SIG_IGN;
    sigemptyset(&new_action.sa_mask);
    new_action.sa_flags = 0;

    int signals[] = {SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGUSR1};
    int n = sizeof(signals)/sizeof(signals[0]);

    for (int i = 0; i < n; i++) {
        int ret = sigaction_syscall(0, signals[i], 0,
                                    (uint64_t)&new_action, 0,
                                    (uint64_t)&old_action, 0,
                                    0, 0, 0, 0, 0, 0);
        printf("[SIGNAL %d] sigaction_syscall returned=%d\n", signals[i], ret);
    }
}

int main() {
    printf("[RUNNING] sigaction_syscall test suite\n");
    test_sigaction_basic();
    test_sigaction_old_only();
    test_sigaction_invalid();
    test_sigaction_all_signals();
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
