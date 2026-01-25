#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>

/* ---------- TEST 1: Basic getpid ---------- */
void test_getpid_basic() {
    pid_t pid = getpid();
    assert(pid > 0);
}

/* ---------- TEST 2: Parent / child PID difference ---------- */
void test_getpid_in_child() {
    pid_t parent_pid = getpid();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        pid_t child_pid = getpid();
        assert(child_pid > 0);
        assert(child_pid != parent_pid);
        _exit(0);
    }

    int status;
    pid_t res = waitpid(pid, &status, 0);
    assert(res == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/* ---------- TEST 3: Uniqueness across multiple children ---------- */
void test_getpid_multiple_children() {
    const int N = 3;
    pid_t pids[N];

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getpid() > 0);
            _exit(0);
        }

        pids[i] = pid;
    }

    /* Parent sees fork return values must be unique */
    assert(pids[0] != pids[1]);
    assert(pids[0] != pids[2]);
    assert(pids[1] != pids[2]);

    for (int i = 0; i < N; i++) {
        int status;
        pid_t res = waitpid(pids[i], &status, 0);
        assert(res == pids[i]);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }
}

/* ---------- TEST 4: Nested forks (parent → child → grandchild) ---------- */
void test_getpid_nested_forks() {
    pid_t parent_pid = getpid();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        pid_t child_pid = getpid();
        assert(child_pid > 0);
        assert(child_pid != parent_pid);

        pid_t gc = fork();
        assert(gc >= 0);

        if (gc == 0) {
            pid_t grandchild_pid = getpid();
            assert(grandchild_pid > 0);
            assert(grandchild_pid != child_pid);
            assert(grandchild_pid != parent_pid);
            _exit(0);
        }

        int st;
        pid_t res = waitpid(gc, &st, 0);
        assert(res == gc);
        assert(WIFEXITED(st));
        assert(WEXITSTATUS(st) == 0);
        _exit(0);
    }

    int st;
    pid_t res = waitpid(pid, &st, 0);
    assert(res == pid);
    assert(WIFEXITED(st));
    assert(WEXITSTATUS(st) == 0);
}

/* ---------- TEST 5: Stress test ---------- */
void test_getpid_stress() {
    const int N = 20;
    pid_t pids[N];

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getpid() > 0);
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

int main() {
    test_getpid_basic();
    test_getpid_in_child();
    test_getpid_multiple_children();
    test_getpid_nested_forks();
    test_getpid_stress();
    return 0;
}
