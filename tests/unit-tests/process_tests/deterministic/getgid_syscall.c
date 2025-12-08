/*
 * getgid() Test Suite
 * -------------------
 * Tests correctness of getgid() across parent and child processes.
 * Covers:
 *   - Basic getgid() call in parent
 *   - Inheritance of real GID after fork
 *   - Multiple children invoking getgid()
 *   - Light stress test with many forks
 */

#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <stdlib.h>
#include <errno.h>

void test_getgid_basic() {
    printf("[TEST 1] getgid in parent\n");
    gid_t gid = getgid();
    printf("[PARENT] getgid returned=%d\n", (int)gid);
}

void test_getgid_in_child() {
    printf("\n[TEST 2] getgid in child\n");
    pid_t pid = fork();
    if (pid == 0) {
        gid_t gid = getgid();
        printf("[CHILD] getgid returned=%d\n", (int)gid);
        _exit(0);
    } else if (pid > 0) {
        int status;
        waitpid(pid, &status, 0);
    } else {
        printf("[ERROR] fork failed\n");
    }
}

void test_getgid_multiple_children() {
    printf("\n[TEST 3] getgid with multiple children\n");
    const int N = 5;
    pid_t pids[N];
    for (int i = 0; i < N; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            printf("[CHILD %d] getgid=%d\n", i, (int)getgid());
            _exit(i);
        }
    }
    for (int i = 0; i < N; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], status);
    }
}

void test_getgid_stress() {
    printf("\n[TEST 4] stress test with 20 children calling getgid\n");
    const int N = 20;
    pid_t pids[N];
    for (int i = 0; i < N; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            for (int j = 0; j < 10; j++) {
                printf("[CHILD %d] call %d getgid=%d\n", i, j, (int)getgid());
            }
            _exit(i);
        }
    }
    for (int i = 0; i < N; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], status);
    }
}

int main() {
    printf("[RUNNING] getgid test suite\n");
    test_getgid_basic();
    test_getgid_in_child();
    test_getgid_multiple_children();
    test_getgid_stress();
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
