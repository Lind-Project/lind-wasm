#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>
#include <stdbool.h>

/* ---------- TEST 1: Basic getgid ---------- */
void test_getgid_basic() {
    gid_t gid = getgid();
    assert(gid >= 0);
}

/* ---------- TEST 2: Inheritance after fork ---------- */
void test_getgid_in_child() {
    gid_t parent_gid = getgid();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        assert(getgid() == parent_gid);
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

/* ---------- TEST 3: Multiple children consistency (order-independent) ---------- */
void test_getgid_multiple_children() {
    const int N = 5;
    gid_t parent_gid = getgid();

    pid_t pids[N];
    bool reaped[N];

    for (int i = 0; i < N; i++) {
        reaped[i] = false;
    }

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getgid() == parent_gid);
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

/* ---------- TEST 4: Stress test ---------- */
void test_getgid_stress() {
    const int CHILDREN = 20;
    const int CALLS = 10;

    gid_t parent_gid = getgid();

    pid_t pids[CHILDREN];
    bool reaped[CHILDREN];

    for (int i = 0; i < CHILDREN; i++) {
        reaped[i] = false;
    }

    for (int i = 0; i < CHILDREN; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            for (int j = 0; j < CALLS; j++) {
                assert(getgid() == parent_gid);
            }
            _exit(0);
        }

        pids[i] = pid;
    }

    int reaped_count = 0;
    int attempts = 0;
    while (reaped_count < CHILDREN) {
        pid_t res = waitpid(-1, NULL, 0);
        attempts++;
        assert(attempts < 100000);

        if (res == -1 && errno == EINTR) {
            continue;
        }
        assert(res > 0);

        bool found = false;
        for (int j = 0; j < CHILDREN; j++) {
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
    test_getgid_basic();
    test_getgid_in_child();
    test_getgid_multiple_children();
    test_getgid_stress();
    return 0;
}
