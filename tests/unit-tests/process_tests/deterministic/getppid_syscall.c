/*
 * getppid() Test Suite
 * --------------------
 * Tests correctness of parent-process identification across forks.
 * Covers:
 *   - Basic getppid() in parent and child
 *   - Multiple children reporting parent PID
 *   - Nested fork behavior (parent → child → grandchild)
 *   - Stress test with many forks
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

void test_getppid_in_parent() {
    printf("[TEST 1] getppid in parent\n");
    int ppid = getppid();
    printf("[PARENT] getppid=%d\n", ppid);
}

void test_getppid_in_child() {
    printf("\n[TEST 2] getppid in child\n");
    pid_t pid = fork();
    if (pid == 0) {
        int ppid = getppid();
        printf("[CHILD] getppid=%d\n", ppid);
        _exit(0);
    }
    int status;
    waitpid(pid, &status, 0);
    printf("[PARENT] child exited, parent pid=%d\n", getppid());
}

void test_getppid_multiple_children() {
    printf("\n[TEST 3] getppid with multiple children\n");
    const int N = 5;
    pid_t pids[N];
    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            int ppid = getppid();
            printf("[CHILD %d] getppid=%d\n", i, ppid);
            _exit(10 + i);
        }
        pids[i] = pid;
    }
    int status;
    for (int i = 0; i < N; i++) {
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d with exit=%d\n", (int)pids[i], WIFEXITED(status) ? WEXITSTATUS(status) : -1);
    }
}

void test_getppid_nested_forks() {
    printf("\n[TEST 4] nested forks\n");
    pid_t pid = fork();
    if (pid == 0) {
        int ppid = getppid();
        printf("[CHILD] getppid=%d\n", ppid);
        pid_t gpid = fork();
        if (gpid == 0) {
            printf("[GRANDCHILD] getppid=%d\n", getppid());
            _exit(0);
        }
        int status;
        waitpid(gpid, &status, 0);
        _exit(0);
    }
    int status;
    waitpid(pid, &status, 0);
}

void test_getppid_stress() {
    printf("\n[TEST 5] stress test with 20 children\n");
    const int N = 20;
    pid_t pids[N];
    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            int ppid = getppid();
            printf("[CHILD %d] getppid=%d\n", i, ppid);
            _exit(i);
        }
        pids[i] = pid;
    }
    int status;
    for (int i = 0; i < N; i++) {
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], WIFEXITED(status) ? WEXITSTATUS(status) : -1);
    }
}

int main() {
    printf("[RUNNING] getppid test suite\n");
    test_getppid_in_parent();
    test_getppid_in_child();
    test_getppid_multiple_children();
    test_getppid_nested_forks();
    test_getppid_stress();
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
