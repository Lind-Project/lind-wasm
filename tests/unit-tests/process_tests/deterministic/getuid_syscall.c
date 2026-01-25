#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>
#include <errno.h>

/* ---------- TEST 1: Basic getuid ---------- */
void test_getuid_basic() {
    uid_t uid = getuid();
    assert(uid >= 0);
}

/* ---------- TEST 2: Inheritance after fork ---------- */
void test_getuid_in_child() {
    uid_t parent_uid = getuid();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        assert(getuid() == parent_uid);
        _exit(0);
    }

    int status;
    pid_t res;
    int attempts = 0;
    do {
        res = waitpid(pid, &status, 0);
        attempts++;
        assert(attempts < 100000);
    } while (res == -1 && errno == EINTR);
    assert(res == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/* ---------- TEST 3: Multiple children consistency ---------- */
void test_getuid_multiple_children() {
    const int N = 4;
    uid_t parent_uid = getuid();

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getuid() == parent_uid);
            _exit(0);
        }
    }

    for (int i = 0; i < N; i++) {
        int status;
        pid_t res;
        int attempts = 0;
        do {
            res = wait(&status);
            attempts++;
            assert(attempts < 100000);
        } while (res == -1 && errno == EINTR);
        assert(res > 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }
}

/* ---------- TEST 4: Stress test ---------- */
void test_getuid_stress() {
    const int CHILDREN = 2;
    const int CALLS = 10;

    uid_t parent_uid = getuid();

    for (int i = 0; i < CHILDREN; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            for (int j = 0; j < CALLS; j++) {
                assert(getuid() == parent_uid);
            }
            _exit(0);
        }
    }

    for (int i = 0; i < CHILDREN; i++) {
        int status;
        pid_t res;
        int attempts = 0;
        do {
            res = wait(&status);
            attempts++;
            assert(attempts < 100000);
        } while (res == -1 && errno == EINTR);
        assert(res > 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }
}

int main() {
    test_getuid_basic();
    test_getuid_in_child();
    test_getuid_multiple_children();
    test_getuid_stress();
    return 0;
}
