#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>
#include <stdbool.h>
#include <errno.h>

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

    pid_t res;
    int attempts = 0;
    do {
        res = waitpid(pid, NULL, 0);
        attempts++;
        assert(attempts < 100000);
    } while (res == -1 && errno == EINTR);
    assert(res == pid);
}

/* ---------- TEST 3: Uniqueness across multiple children ---------- */
void test_getpid_multiple_children() {
    enum { N = 3 };
    pid_t pids[N];
    bool reaped[N] = { false };

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getpid() > 0);
            _exit(0);
        }

        pids[i] = pid;
    }

    /* fork() return values must be unique */
    for (int i = 0; i < N; i++) {
        for (int j = i + 1; j < N; j++) {
            assert(pids[i] != pids[j]);
        }
    }

    /* Reap children in any order */
    int reaped_count = 0;
    int attempts = 0;
    while (reaped_count < N) {
        pid_t res = waitpid(-1, NULL, 0);
        attempts++;
        assert(attempts < 100000);

        if (res == -1 && errno == EINTR) {
            continue;
        }
        assert(res > 0);

        bool found = false;
        for (int j = 0; j < N; j++) {
            if (res == pids[j]) {
                assert(!reaped[j]);
                reaped[j] = true;
                found = true;
                reaped_count++;
                break;
            }
        }
        assert(found);
    }
}

/* ---------- TEST 4: Nested forks ---------- */
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

        pid_t r;
        int attempts = 0;
        do {
            r = waitpid(gc, NULL, 0);
            attempts++;
            assert(attempts < 100000);
        } while (r == -1 && errno == EINTR);
        assert(r == gc);
        _exit(0);
    }

    pid_t r;
    int attempts = 0;
    do {
        r = waitpid(pid, NULL, 0);
        attempts++;
        assert(attempts < 100000);
    } while (r == -1 && errno == EINTR);
    assert(r == pid);
}

/* ---------- TEST 5: Stress test (order-independent) ---------- */
void test_getpid_stress() {
    enum { N = 20 };
    pid_t pids[N];
    bool reaped[N] = { false };

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getpid() > 0);
            _exit(0);
        }

        pids[i] = pid;
    }

    int reaped_count = 0;
    int attempts = 0;
    while (reaped_count < N) {
        pid_t res = waitpid(-1, NULL, 0);
        attempts++;
        assert(attempts < 100000);

        if (res == -1 && errno == EINTR) {
            continue;
        }

        assert(res > 0);

        bool found = false;
        for (int j = 0; j < N; j++) {
            if (res == pids[j]) {
                assert(!reaped[j]);
                reaped[j] = true;
                found = true;
                reaped_count++;
                break;
            }
        }
        assert(found);
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