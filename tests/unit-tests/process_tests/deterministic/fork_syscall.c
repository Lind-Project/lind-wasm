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

    if (pid == 0) {
        printf("[CHILD] PID=%d PPID=%d\n", getpid(), getppid());
        _exit(0);
    }

    if (pid < 0) {
        printf("[FAIL] fork failed errno=%d\n", errno);
        return;
    }

    printf("[PARENT] PID=%d CHILD=%d\n", getpid(), pid);
    waitpid(pid, NULL, 0);
    printf("[PARENT] Child finished\n");
}

void test_memory_isolation() {
    printf("\n[TEST 2] Memory isolation\n");

    int x = 10;
    pid_t pid = fork();

    if (pid == 0) {
        printf("[CHILD] x(before)=%d\n", x);
        x = 999;
        printf("[CHILD] x(after)=%d\n", x);
        _exit(0);
    }

    waitpid(pid, NULL, 0);
    printf("[PARENT] x=%d (should remain 10)\n", x);
}

void test_uid_gid() {
    printf("\n[TEST 3] UID/GID inheritance\n");

    printf("[PARENT] UID=%d GID=%d EUID=%d EGID=%d\n",
           getuid(), getgid(), geteuid(), getegid());

    pid_t pid = fork();
    if (pid == 0) {
        printf("[CHILD] UID=%d GID=%d\n", getuid(), getgid());
        _exit(0);
    }

    waitpid(pid, NULL, 0);
}

void test_waitpid_nohang() {
    printf("\n[TEST 4] waitpid WNOHANG\n");

    pid_t pid = fork();
    if (pid == 0) {
        sleep(1);
        _exit(0);
    }

    int status = 0;
    pid_t res = waitpid(pid, &status, WNOHANG);
    printf("[PARENT] WNOHANG result=%d (0 means child not exited)\n", res);

    waitpid(pid, &status, 0);
    printf("[PARENT] Child later exited normally\n");
}

void test_zombie_behavior() {
    printf("\n[TEST 5] Zombie behavior\n");

    pid_t pid = fork();
    if (pid == 0) {
        printf("[CHILD] Exiting to become zombie\n");
        _exit(0);
    }

    sleep(1);

    int status = 0;
    pid_t res = waitpid(pid, &status, 0);
    printf("[PARENT] waitpid result=%d (cleaned zombie)\n", res);
}

void test_orphan_adoption() {
    printf("\n[TEST 6] Orphan adoption\n");

    pid_t pid = fork();
    if (pid == 0) {
        sleep(1);
        printf("[CHILD] After parent exit PPID=%d (should be 1)\n", getppid());
        _exit(0);
    }

    printf("[PARENT] Exiting early to orphan child\n");
    _exit(0);
}

void test_orphan_adoption_wrapper() {
    pid_t pid = fork();
    if (pid == 0) {
        test_orphan_adoption();
        _exit(0);
    }
    waitpid(pid, NULL, 0);
}

void test_multiple_children() {
    printf("\n[TEST 7] Multiple children\n");

    for (int i = 0; i < 3; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            printf("[CHILD %d] PID=%d\n", i, getpid());
            _exit(0);
        }
    }

    int status;
    while (wait(&status) > 0) {
        printf("[PARENT] A child exited\n");
    }
}

void test_pipe_fork() {
    printf("\n[TEST 8] Pipe + fork communication\n");

    int fds[2];
    pipe(fds);

    pid_t pid = fork();

    if (pid == 0) {
        close(fds[1]);

        char buf[32];
        ssize_t n = read(fds[0], buf, sizeof(buf));
        printf("[CHILD] read returned %d errno=%d\n", (int)n, errno);
        if (n > 0) buf[n] = '\0';
        printf("[CHILD] message='%s'\n", buf);

        close(fds[0]);
        _exit(0);
    }

    close(fds[0]);
    const char* msg = "hello_from_parent";
    ssize_t n = write(fds[1], msg, strlen(msg));
    printf("[PARENT] write returned %d\n", (int)n);
    close(fds[1]);

    waitpid(pid, NULL, 0);
}

void stress_test_fork_chain() {
    printf("\n[STRESS 2] Fork chain depth 10\n");
    fflush(stdout);

    pid_t pid;
    int depth = 0;
    int is_child = 0;

    for (int i = 0; i < 10; i++) {
        pid = fork();

        if (pid < 0) {
            if (!is_child)
                printf("[FAIL] fork failed at depth %d\n", i);
            return;
        }

        if (pid == 0) {
            is_child = 1;
            depth++;
            continue;
        }

        waitpid(pid, NULL, 0);
        printf("[OK] Fork chain completed. Final depth=%d\n", depth);
        break;
    }
    _exit(0);
}

void stress_test_multiple_small_forks() {
    printf("\n[STRESS 1] Create 10 sequential children\n");
    fflush(stdout);

    for (int i = 0; i < 10; i++) {
        pid_t pid = fork();
        if (pid < 0) {
            printf("[FAIL] fork failed at iteration %d\n", i);
            return;
        }
        if (pid == 0) _exit(0);
        waitpid(pid, NULL, 0);
    }

    printf("[OK] All 10 children forked and reaped successfully\n");
}



int main() {
    printf("[RUNNING] Extended fork-only test suite\n");

    test_basic_fork();
    test_memory_isolation();
    test_uid_gid();
    test_waitpid_nohang();
    test_zombie_behavior();
    test_orphan_adoption_wrapper();
    test_multiple_children();
    test_pipe_fork();
    stress_test_multiple_small_forks();
    stress_test_fork_chain();
    

    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
