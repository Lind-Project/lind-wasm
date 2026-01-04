/*
 * getegid() Test Suite
 * --------------------
 * Tests correct behavior of getegid() across parent/child processes.
 * Covers:
 *   - Basic getegid() call
 *   - Inheritance of effective GID after fork
 *   - Multiple children calling getegid()
 *   - Stress test with many rapid forks
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>

void test_getegid_basic() {
    printf("[TEST 1] getegid in parent\n");
    gid_t gid = getegid();
    printf("[PARENT] getegid returned=%d\n", (int)gid);
}

void test_getegid_in_child() {
    printf("\n[TEST 2] getegid in child\n");
    pid_t pid = fork();
    if (pid == 0) {
        gid_t child_gid = getegid();
        printf("[CHILD] getegid returned=%d\n", (int)child_gid);
        _exit(0);
    } else if (pid > 0) {
        int status;
        waitpid(pid, &status, 0);
        printf("[PARENT] child exited, parent getegid=%d\n", (int)getegid());
    } else {
        printf("[ERROR] fork failed\n");
    }
}

void test_getegid_multiple_children() {
    printf("\n[TEST 3] getegid with multiple children\n");
    pid_t pids[4];
    for (int i = 0; i < 4; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            printf("[CHILD %d] getegid=%d\n", i, (int)getegid());
            _exit(i);
        }
    }

    for (int i = 0; i < 4; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], status);
    }
}

void test_getegid_stress() {
    printf("\n[TEST 4] Stress test with 20 children calling getegid\n");
    pid_t pids[20];

    for (int i = 0; i < 20; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            for (int j = 0; j < 10; j++) {
                printf("[CHILD %d] call %d getegid=%d\n", i, j, (int)getegid());
            }
            _exit(i);
        }
    }

    for (int i = 0; i < 20; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], status);
    }
}

int main() {
    printf("[RUNNING] getegid test suite\n");
    test_getegid_basic();
    test_getegid_in_child();
    test_getegid_multiple_children();
    test_getegid_stress();
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
