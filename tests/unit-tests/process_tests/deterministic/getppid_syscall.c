#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <stdlib.h>
#include <errno.h>
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
    if (pid == 0) {
        x = 999;
        printf("[CHILD] x changed to %d\n", x);
        _exit(0);
    }
    waitpid(pid, NULL, 0);
    printf("[PARENT] x=%d (should remain 10)\n", x);
}

void test_multiple_children() {
    printf("\n[TEST 3] Multiple children\n");
    for (int i = 0; i < 3; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            printf("[CHILD %d] executed\n", i);
            _exit(0);
        }
        waitpid(pid, NULL, 0);
        printf("[PARENT] reaped child %d\n", i);
    }
}

void test_pipe_communication() {
    printf("\n[TEST 4] Pipe communication\n");
    int fds[2];
    pipe(fds);
    pid_t pid = fork();
    if (pid == 0) {
        close(fds[1]);
        char buf[32];
        ssize_t n = read(fds[0], buf, sizeof(buf));
        if (n > 0) buf[n] = '\0';
        printf("[CHILD] received message='%s'\n", buf);
        close(fds[0]);
        _exit(0);
    }
    close(fds[0]);
    const char* msg = "hello_from_parent";
    write(fds[1], msg, strlen(msg));
    close(fds[1]);
    waitpid(pid, NULL, 0);
    printf("[PARENT] pipe test done\n");
}

void stress_test_sequential_forks() {
    printf("\n[TEST 5] Stress: 10 sequential forks\n");
    for (int i = 0; i < 10; i++) {
        pid_t pid = fork();
        if (pid == 0) _exit(0);
        waitpid(pid, NULL, 0);
        printf("[PARENT] reaped child %d\n", i);
    }
}

int main() {
    test_basic_fork();
    test_memory_isolation();
    test_multiple_children();
    test_pipe_communication();
    stress_test_sequential_forks();
    return 0;
}
