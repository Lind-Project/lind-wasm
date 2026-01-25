#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>
#include <string.h>

/* ---------- TEST 1: Basic fork ---------- */
void test_basic_fork() {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        _exit(0);
    }

    int status;
    pid_t res = waitpid(pid, &status, 0);
    assert(res == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/* ---------- TEST 2: Memory isolation ---------- */
void test_memory_isolation() {
    int x = 10;
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        x = 999;
        _exit(0);
    }

    waitpid(pid, NULL, 0);
    assert(x == 10);
}

/* ---------- TEST 3: Multiple children ---------- */
void test_multiple_children() {
    const int N = 3;
    pid_t pids[N];

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            _exit(0);
        }

        pids[i] = pid;
    }

    for (int i = 0; i < N; i++) {
        int status;
        pid_t res = waitpid(pids[i], &status, 0);
        assert(res == pids[i]);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }
}

/* ---------- TEST 4: Pipe communication ---------- */
void test_pipe_communication() {
    int fds[2];
    assert(pipe(fds) == 0);

    const char *msg = "hello_from_parent";
    size_t len = strlen(msg);

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        close(fds[1]);

        char buf[64];
        ssize_t n = read(fds[0], buf, sizeof(buf));
        assert(n == (ssize_t)len);
        assert(memcmp(buf, msg, len) == 0);

        close(fds[0]);
        _exit(0);
    }

    close(fds[0]);
    assert(write(fds[1], msg, len) == (ssize_t)len);
    close(fds[1]);

    int status;
    pid_t res = waitpid(pid, &status, 0);
    assert(res == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/* ---------- TEST 5: Stress: sequential forks ---------- */
void stress_test_sequential_forks() {
    const int N = 10;

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            _exit(0);
        }

        int status;
        pid_t res = waitpid(pid, &status, 0);
        assert(res == pid);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
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
