#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>
#include <errno.h>

/* ---------- TEST 1: Basic getegid ---------- */
void test_getegid_basic() {
    gid_t gid = getegid();
    assert(gid >= 0);
}

/* ---------- TEST 2: Inheritance after fork ---------- */
void test_getegid_in_child() {
    gid_t parent_gid = getegid();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        assert(getegid() == parent_gid);
        _exit(0);
    }

    int status;
    pid_t res;
    do {
        res = waitpid(pid, &status, 0);
    } while (res == -1 && errno == EINTR);

    assert(res == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/* ---------- TEST 3: Multiple children consistency ---------- */
void test_getegid_multiple_children() {
    const int N = 4;
    gid_t parent_gid = getegid();

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getegid() == parent_gid);
            _exit(0);
        }
    }

    int reaped = 0;
    while (reaped < N) {
        int status;
        pid_t res = waitpid(-1, &status, 0);

        if (res > 0) {
            assert(WIFEXITED(status));
            assert(WEXITSTATUS(status) == 0);
            reaped++;
        } else if (res == -1 && errno == ECHILD) {
            /* Lind-wasm may report ECHILD early — break safely */
            break;
        }
    }

    assert(reaped == N);
}

/* ---------- TEST 4: Stress test ---------- */
void test_getegid_stress() {
    const int CHILDREN = 20;
    const int CALLS = 10;

    gid_t parent_gid = getegid();

    for (int i = 0; i < CHILDREN; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            for (int j = 0; j < CALLS; j++) {
                assert(getegid() == parent_gid);
            }
            _exit(0);
        }
    }

    int reaped = 0;
    while (reaped < CHILDREN) {
        int status;
        pid_t res = waitpid(-1, &status, 0);

        if (res > 0) {
            assert(WIFEXITED(status));
            assert(WEXITSTATUS(status) == 0);
            reaped++;
        } else if (res == -1 && errno == ECHILD) {
            break;
        }
    }

    assert(reaped == CHILDREN);
}

int main() {
    test_getegid_basic();
    test_getegid_in_child();
    test_getegid_multiple_children();
    test_getegid_stress();
    return 0;
}
