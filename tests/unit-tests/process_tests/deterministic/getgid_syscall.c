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
    printf("[TEST 1] getgid basic\n");

    gid_t gid = getgid();
    if (gid >= 0)
        printf("[OK] getgid returned a valid value\n");
    else
        printf("[FAIL] getgid failed\n");
}

void test_getgid_in_child() {
    printf("\n[TEST 2] getgid inheritance\n");

    gid_t parent_gid = getgid();
    pid_t pid = fork();

    if (pid == 0) {
        gid_t child_gid = getgid();
        if (child_gid == parent_gid)
            printf("[OK] child inherited gid\n");
        else
            printf("[FAIL] child gid mismatch\n");
        _exit(0);
    }

    waitpid(pid, NULL, 0);
}

void test_getgid_multiple_children() {
    printf("\n[TEST 3] getgid consistency across children\n");

    gid_t parent_gid = getgid();

    for (int i = 0; i < 5; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            if (getgid() == parent_gid)
                _exit(0);
            else
                _exit(1);
        }
    }

    int ok = 1;
    for (int i = 0; i < 5; i++) {
        int status;
        wait(&status);
        if (WEXITSTATUS(status) != 0)
            ok = 0;
    }

    if (ok)
        printf("[OK] all children inherited gid\n");
    else
        printf("[FAIL] gid mismatch in children\n");
}

void test_getgid_stress() {
    printf("\n[TEST 4] getgid stress\n");

    gid_t parent_gid = getgid();

    for (int i = 0; i < 20; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            for (int j = 0; j < 10; j++) {
                if (getgid() != parent_gid)
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
    printf("[RUNNING] getgid test suite\n");
    test_getgid_basic();
    test_getgid_in_child();
    test_getgid_multiple_children();
    test_getgid_stress();
    return 0;
}
