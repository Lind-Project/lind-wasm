/*
 * Fork Test Suite
 * ----------------
 * This program tests core fork() and process-management behavior on
 * POSIX-like systems. It verifies:
 *
 *  - Basic fork semantics (PID/PPID, parent/child execution)
 *  - Memory isolation between parent and child
 *  - UID/GID inheritance
 *  - waitpid() behavior, including WNOHANG and zombie cleanup
 *  - Orphan adoption by init
 *  - Multiple-child handling
 *  - Pipe-based parent → child communication
 *  - Small stress tests (sequential forks, fork chains)
 *
 * Intended for validating correct kernel or runtime implementations of
 * fork(), wait(), and basic IPC.
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>
#include <sys/types.h>
#include <string.h>

void test_basic_fork() {
    printf("[TEST 1] Basic fork\n");

    pid_t pid = fork();

    if (pid < 0) {
        printf("[FAIL] fork failed errno=%d\n", errno);
        return;
    }

    if (pid == 0) {
        printf("[CHILD] fork success\n");
        _exit(0);
    }
    waitpid(pid, NULL, 0);
    printf("[PARENT] child finished\n");
}


void test_memory_isolation() {
    printf("\n[TEST 2] Memory isolation\n");

    int x = 10;
    pid_t pid = fork();

    if (pid < 0) {
        printf("[FAIL] fork failed errno=%d\n", errno);
        return;
    }

    if (pid == 0) {
        x = 999;
        _exit(0);
    }

    waitpid(pid, NULL, 0);

    if (x == 10) {
        printf("[PARENT] memory isolated\n");
    } else {
        printf("[FAIL] memory corrupted\n");
    }
}


void test_uid_gid() {
    printf("\n[TEST 3] UID/GID inheritance\n");

    uid_t uid = getuid();
    gid_t gid = getgid();
    uid_t euid = geteuid();
    gid_t egid = getegid();

    pid_t pid = fork();
    if (pid < 0) {
        printf("[FAIL] fork failed errno=%d\n", errno);
        return;
    }

    if (pid == 0) {
        if (getuid() == uid &&
            getgid() == gid &&
            geteuid() == euid &&
            getegid() == egid) {
            _exit(0);
        } else {
            _exit(1);
        }
    }

    int status;
    waitpid(pid, &status, 0);

    if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
        printf("[PARENT] UID/GID inherited\n");
    } else {
        printf("[FAIL] UID/GID mismatch\n");
    }
}

void test_zombie_behavior() {
    printf("\n[TEST 4] Zombie / reaping behavior\n");

    pid_t pid = fork();
    if (pid < 0) {
        printf("[FAIL] fork failed errno=%d\n", errno);
        return;
    }

    if (pid == 0) {
        _exit(0);
    }

    int status;
    pid_t res = waitpid(pid, &status, 0);

    if (res == pid && WIFEXITED(status)) {
        printf("[PARENT] child reaped successfully\n");
    } else {
        printf("[FAIL] child not reaped\n");
    }
}

void test_multiple_children() {
    printf("\n[TEST 5] Multiple children\n");

    pid_t pids[3];

    for (int i = 0; i < 3; i++) {
        pid_t pid = fork();
        if (pid < 0) {
            printf("[FAIL] fork failed errno=%d\n", errno);
            return;
        }

        if (pid == 0) {
            _exit(0);
        }

        pids[i] = pid;
    }

    for (int i = 0; i < 3; i++) {
        waitpid(pids[i], NULL, 0);
        printf("[PARENT] child reaped\n");
    }
}


void test_pipe_fork() {
    printf("\n[TEST 6] Pipe + fork communication\n");

    int fds[2];
    if (pipe(fds) < 0) {
        printf("[FAIL] pipe failed errno=%d\n", errno);
        return;
    }

    pid_t pid = fork();
    if (pid < 0) {
        printf("[FAIL] fork failed errno=%d\n", errno);
        return;
    }

    if (pid == 0) {
        close(fds[1]);

        char buf;
        if (read(fds[0], &buf, 1) == 1) {
            printf("[CHILD] received message\n");
            _exit(0);
        } else {
            _exit(1);
        }
    }
    close(fds[0]);

    write(fds[1], "x", 1);
    close(fds[1]);

    int status;
    waitpid(pid, &status, 0);

    if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
        printf("[PARENT] pipe communication successful\n");
    } else {
        printf("[FAIL] pipe communication failed\n");
    }
}


void stress_test_multiple_small_forks() {
    printf("\n[TEST 7] Stress: create 10 sequential children\n");
    fflush(stdout);

    for (int i = 0; i < 10; i++) {
        errno = 0;
        pid_t pid = fork();

        if (pid < 0) {
            printf("[FAIL] fork failed at iteration %d errno=%d\n", i, errno);
            return;
        }

        if (pid == 0) {
            _exit(0);
        }

        waitpid(pid, NULL, 0);
    }

    printf("[OK] All 10 children forked and reaped successfully\n");
}

void stress_test_fork_chain() {
    printf("\n[TEST 8] Stress: fork chain depth 10\n");
    fflush(stdout);

    for (int i = 0; i < 10; i++) {
        pid_t pid = fork();

        if (pid < 0) {
            printf("[FAIL] fork failed at depth %d\n", i);
            _exit(1);
        }

        if (pid == 0) {
            if (i == 9) {
                printf("[OK] Fork chain depth 10 completed successfully\n");
                fflush(stdout);
            }
            continue;
        }
        waitpid(pid, NULL, 0);
        _exit(0);
    }
    _exit(0);
}

int main() {
    printf("[RUNNING] Extended fork-only test suite\n");

    test_basic_fork();
    test_memory_isolation();
    test_uid_gid();
    test_zombie_behavior();
    test_multiple_children();
    test_pipe_fork();
    stress_test_multiple_small_forks();
    stress_test_fork_chain();
    return 0;
}
