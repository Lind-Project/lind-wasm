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
    printf("[TEST 1] getegid basic\n");

    gid_t gid = getegid();
    if (gid >= 0)
        printf("[OK] getegid returned a valid value\n");
    else
        printf("[FAIL] getegid failed\n");
}

void test_getegid_in_child() {
    printf("\n[TEST 2] getegid inheritance\n");

    gid_t parent_gid = getegid();
    pid_t pid = fork();

    if (pid == 0) {
        gid_t child_gid = getegid();
        if (child_gid == parent_gid)
            printf("[OK] child inherited egid\n");
        else
            printf("[FAIL] child egid mismatch\n");
        _exit(0);
    }

    waitpid(pid, NULL, 0);
}

void test_getegid_multiple_children() {
    printf("\n[TEST 3] getegid consistency across children\n");

    gid_t parent_gid = getegid();

    for (int i = 0; i < 4; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            if (getegid() == parent_gid)
                _exit(0);
            else
                _exit(1);
        }
    }

    int ok = 1;
    for (int i = 0; i < 4; i++) {
        int status;
        wait(&status);
        if (WEXITSTATUS(status) != 0)
            ok = 0;
    }

    if (ok)
        printf("[OK] all children inherited egid\n");
    else
        printf("[FAIL] egid mismatch in children\n");
}

void test_getegid_stress() {
    printf("\n[TEST 4] getegid stress\n");

    gid_t parent_gid = getegid();

    for (int i = 0; i < 20; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            for (int j = 0; j < 10; j++) {
                if (getegid() != parent_gid)
                    _exit(1);
            }
            _exit(0);
        }
    }

    int ok = 1;
    for (int i = 0; i < 20; i++) {
        int status;
        wait(&status);
        if (WEXITSTATUS(status) != 0)
            ok = 0;
    }

    if (ok)
        printf("[OK] stress test passed\n");
    else
        printf("[FAIL] stress test failed\n");
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
