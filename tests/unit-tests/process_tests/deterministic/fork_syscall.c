#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>

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
    int x = 42;
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        x = 999;
        _exit(0);
    }

    waitpid(pid, NULL, 0);
    assert(x == 42);
}

/* ---------- TEST 3: UID / GID inheritance ---------- */
void test_uid_gid() {
    uid_t uid = getuid();
    gid_t gid = getgid();
    uid_t euid = geteuid();
    gid_t egid = getegid();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        assert(getuid() == uid);
        assert(getgid() == gid);
        assert(geteuid() == euid);
        assert(getegid() == egid);
        _exit(0);
    }

    waitpid(pid, NULL, 0);
}

/* ---------- TEST 4: Zombie reaping ---------- */
void test_zombie_reaping() {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        _exit(0);
    }

    int status;
    pid_t res = waitpid(pid, &status, 0);
    assert(res == pid);
    assert(WIFEXITED(status));
}

/* ---------- TEST 5: Multiple children (Lind-safe) ---------- */
void test_multiple_children() {
    const int N = 3;

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            _exit(0);
        }
    }

    for (int i = 0; i < N; i++) {
        int status;
        pid_t res = waitpid(-1, &status, 0);
        assert(res > 0);
        assert(WIFEXITED(status));
    }
}

/* ---------- TEST 6: Pipe + fork IPC ---------- */
void test_pipe_fork() {
    int fds[2];
    assert(pipe(fds) == 0);

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        close(fds[1]);
        char c;
        ssize_t r = read(fds[0], &c, 1);
        assert(r == 1);
        close(fds[0]);
        _exit(0);
    }

    close(fds[0]);
    assert(write(fds[1], "x", 1) == 1);
    close(fds[1]);

    waitpid(pid, NULL, 0);
}

/* ---------- TEST 7: Sequential fork stress ---------- */
void stress_test_sequential_forks() {
    for (int i = 0; i < 10; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            _exit(0);
        }

        waitpid(pid, NULL, 0);
    }
}

/* ---------- TEST 8: Fork chain (deterministic) ---------- */
void stress_test_fork_chain() {
    const int DEPTH = 8;

    for (int i = 0; i < DEPTH; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            if (i == DEPTH - 1) {
                _exit(0);
            }
            continue;
        }

        waitpid(pid, NULL, 0);
        _exit(0);
    }

    _exit(0);
}

int main() {
    test_basic_fork();
    test_memory_isolation();
    test_uid_gid();
    test_zombie_reaping();
    test_multiple_children();
    test_pipe_fork();
    stress_test_sequential_forks();
    stress_test_fork_chain();
    return 0;
}
