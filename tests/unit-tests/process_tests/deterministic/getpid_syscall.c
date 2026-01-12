/*
 * getpid() Test Suite
 * -------------------
 * Verifies correct getpid() behavior across forked processes.
 * Covers:
 *   - Basic getpid() in parent and child
 *   - Multiple children calling getpid()
 *   - Nested fork (parent → child → grandchild)
 *   - Stress test with many rapid forks
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>

void test_getpid_basic() {
    printf("[TEST 1] getpid basic\n");

    pid_t pid = getpid();
    if (pid > 0)
        printf("[OK] getpid returned valid pid\n");
    else
        printf("[FAIL] getpid returned invalid pid\n");
}

void test_getpid_in_child() {
    printf("\n[TEST 2] getpid parent/child difference\n");

    pid_t parent_pid = getpid();
    pid_t pid = fork();

    if (pid == 0) {
        pid_t child_pid = getpid();
        if (child_pid != parent_pid)
            printf("[OK] child pid differs from parent\n");
        else
            printf("[FAIL] child pid equals parent\n");
        _exit(0);
    }

    waitpid(pid, NULL, 0);
}

void test_getpid_multiple_children() {
    printf("\n[TEST 3] getpid uniqueness across children\n");

    pid_t pids[3];

    for (int i = 0; i < 3; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            _exit(0);
        }
        pids[i] = pid;
    }

    int unique = 1;
    if (pids[0] == pids[1] || pids[1] == pids[2] || pids[0] == pids[2])
        unique = 0;

    for (int i = 0; i < 3; i++)
        waitpid(pids[i], NULL, 0);

    if (unique)
        printf("[OK] children received unique pids\n");
    else
        printf("[FAIL] pid collision detected\n");
}

void test_getpid_nested_forks() {
    printf("\n[TEST 4] nested fork pid validity\n");

    pid_t parent_pid = getpid();
    pid_t pid = fork();

    if (pid == 0) {
        pid_t child_pid = getpid();
        pid_t gc = fork();

        if (gc == 0) {
            pid_t grandchild_pid = getpid();
            if (grandchild_pid != child_pid &&
                grandchild_pid != parent_pid)
                _exit(0);
            else
                _exit(1);
        }

        int st;
        waitpid(gc, &st, 0);
        _exit(WEXITSTATUS(st));
    }

    int st;
    waitpid(pid, &st, 0);

    if (WEXITSTATUS(st) == 0)
        printf("[OK] nested fork pids valid\n");
    else
        printf("[FAIL] nested fork pid error\n");
}

void test_getpid_stress() {
    printf("\n[TEST 5] getpid stress\n");

    const int N = 20;
    pid_t pids[N];
    int ok = 1;

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            if (getpid() > 0)
                _exit(0);
            else
                _exit(1);
        }
        pids[i] = pid;
    }

    for (int i = 0; i < N; i++) {
        int st;
        waitpid(pids[i], &st, 0);
        if (WEXITSTATUS(st) != 0)
            ok = 0;
    }

    if (ok)
        printf("[OK] stress test passed\n");
    else
        printf("[FAIL] stress test failed\n");
}

int main() {
    printf("[RUNNING] getpid test suite\n");
    test_getpid_basic();
    test_getpid_in_child();
    test_getpid_multiple_children();
    test_getpid_nested_forks();
    test_getpid_stress();
    return 0;
}
