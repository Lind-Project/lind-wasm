#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <errno.h>
#include <signal.h>

void test_basic_exec() {
    printf("[TEST 1] Basic exec\n");

    pid_t pid = fork();
    if (pid == 0) {
        char *args[] = { "/bin/echo", "exec_ok", NULL };
        execvp(args[0], args);
        printf("[CHILD] exec failed errno=%d\n", errno);
        _exit(1);
    }

    int st = 0;
    waitpid(pid, &st, 0);
    printf("[PARENT] Child exited=%d\n", WEXITSTATUS(st));
}

void test_memory_reset() {
    printf("\n[TEST 2] Memory reset\n");

    static int x = 10;

    pid_t pid = fork();
    if (pid == 0) {
        x = 12345;
        char *args[] = { "/bin/echo", "mem_reset", NULL };
        execvp(args[0], args);
        printf("[CHILD] exec failed errno=%d\n", errno);
        _exit(1);
    }

    waitpid(pid, NULL, 0);

    printf("[PARENT] x=%d\n", x);
}

void test_fd_cloexec() {
    printf("\n[TEST 3] FD_CLOEXEC\n");

    int fd = open("/etc/hostname", O_RDONLY);
    fcntl(fd, F_SETFD, FD_CLOEXEC);

    pid_t pid = fork();
    if (pid == 0) {
        char buf[16];
        int n = read(fd, buf, sizeof(buf));
        printf("[CHILD] read=%d errno=%d (should not read)\n", n, errno);
        char *args[] = { "/bin/echo", "cloexec", NULL };
        execvp(args[0], args);
        _exit(1);
    }

    waitpid(pid, NULL, 0);
    close(fd);
}

void test_fd_inherit() {
    printf("\n[TEST 4] FD inherit\n");

    int fds[2];
    pipe(fds);

    pid_t pid = fork();
    if (pid == 0) {
        close(fds[1]);
        char *args[] = { "/bin/cat", NULL };
        dup2(fds[0], 0);
        execvp(args[0], args);
        printf("[CHILD] exec failed errno=%d\n", errno);
        _exit(1);
    }

    close(fds[0]);
    write(fds[1], "hello\n", 6);
    close(fds[1]);

    waitpid(pid, NULL, 0);
}

void test_pid_preserved() {
    printf("\n[TEST 5] PID preserved\n");

    pid_t pid = fork();
    if (pid == 0) {
        printf("[CHILD-before] pid=%d\n", getpid());
        char *args[] = { "/bin/sh", "-c", "echo [CHILD-after] pid=$$", NULL };
        execvp(args[0], args);
        _exit(1);
    }

    waitpid(pid, NULL, 0);
}

void test_signal_reset() {
    printf("\n[TEST 6] Signal reset\n");

    signal(SIGINT, SIG_IGN);

    pid_t pid = fork();
    if (pid == 0) {
        char *args[] = { "/bin/sh", "-c", "kill -s INT $$; echo signal_alive", NULL };
        execvp(args[0], args);
        _exit(1);
    }

    waitpid(pid, NULL, 0);
}

void test_args_env() {
    printf("\n[TEST 7] Args & env\n");

    pid_t pid = fork();
    if (pid == 0) {
        char *args[] = { "/usr/bin/env", NULL };
        execvp(args[0], args);
        printf("[CHILD] exec failed errno=%d\n", errno);
        _exit(1);
    }

    waitpid(pid, NULL, 0);
}

void test_cwd_persist() {
    printf("\n[TEST 8] CWD persist\n");

    chdir("/tmp");

    pid_t pid = fork();
    if (pid == 0) {
        char *args[] = { "/bin/pwd", NULL };
        execvp(args[0], args);
        _exit(1);
    }

    waitpid(pid, NULL, 0);
}

void test_exec_failure() {
    printf("\n[TEST 9] Exec failure\n");

    pid_t pid = fork();
    if (pid == 0) {
        char *args[] = { "/no/such/file", NULL };
        execvp(args[0], args);
        printf("[CHILD] exec expectedly failed errno=%d\n", errno);
        _exit(0);
    }

    waitpid(pid, NULL, 0);
}

void test_stress_exec() {
    printf("\n[TEST 10] Stress exec\n");

    for (int i = 0; i < 5; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            char *args[] = { "/bin/echo", "stress", NULL };
            execvp(args[0], args);
            _exit(1);
        }
        waitpid(pid, NULL, 0);
    }
}

int main() {
    printf("[RUNNING] exec test suite\n");

    test_basic_exec();
    test_memory_reset();
    test_fd_cloexec();
    test_fd_inherit();
    test_pid_preserved();
    test_signal_reset();
    test_args_env();
    test_cwd_persist();
    test_exec_failure();
    test_stress_exec();

    printf("\n[DONE]\n");
    return 0;
}
