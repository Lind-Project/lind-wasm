#define _GNU_SOURCE
#include <assert.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>

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

    int status;
    pid_t res = waitpid(pid, &status, 0);
    assert(res == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/* ---------- TEST 3: Multiple children consistency ---------- */
void test_getgid_multiple_children() {
    const int N = 5;
    gid_t parent_gid = getgid();

    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            assert(getgid() == parent_gid);
            _exit(0);
        }
    }

    for (int i = 0; i < N; i++) {
        int status;
        pid_t res = wait(&status);
        assert(res > 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }
}

/* ---------- TEST 4: Stress test ---------- */
void test_getgid_stress() {
    const int CHILDREN = 20;
    const int CALLS = 10;

    gid_t parent_gid = getgid();

    for (int i = 0; i < CHILDREN; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            for (int j = 0; j < CALLS; j++) {
                assert(getgid() == parent_gid);
            }
            _exit(0);
        }
    }

    for (int i = 0; i < CHILDREN; i++) {
        int status;
        pid_t res = wait(&status);
        assert(res > 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }
}

int main() {
    test_getgid_basic();
    test_getgid_in_child();
    test_getgid_multiple_children();
    test_getgid_stress();
    return 0;
}
