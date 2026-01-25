#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>
#include <assert.h>

/*
 * TEST 1: Basic waitpid on a specific child
 */
void test_waitpid_basic() {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        _exit(1);
    }

    int status;
    pid_t r = waitpid(pid, &status, 0);

    assert(r == pid);
}

/*
 * TEST 2: waitpid with WNOHANG
 */
void test_waitpid_wnohang() {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        sleep(1);
        _exit(0);
    }

    int status;
    pid_t r = waitpid(pid, &status, WNOHANG);

    /* Either child not exited yet (0) or already reaped (pid) */
    assert(r == 0 || r == pid);

    /* Ensure the child is eventually reaped */
    if (r == 0) {
        r = waitpid(pid, &status, 0);
        assert(r == pid);
    }

    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);
}

/*
 * TEST 3: waitpid with NULL status
 */
void test_waitpid_status_null() {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        _exit(0);
    }

    pid_t r = waitpid(pid, NULL, 0);
    assert(r == pid);
}

/*
 * TEST 4: waitpid when no children exist
 */
void test_waitpid_no_children() {
    int status;
    errno = 0;

    pid_t r = waitpid(-1, &status, WNOHANG);

    assert(r == -1);
    assert(errno == ECHILD);
}

/*
 * TEST 5: Multiple sequential children
 */
void test_waitpid_multiple_sequential() {
    for (int i = 0; i < 3; i++) {
        pid_t pid = fork();
        assert(pid >= 0);

        if (pid == 0) {
            _exit(0);
        }

        pid_t r = waitpid(pid, NULL, 0);
        assert(r == pid);
    }
}

int main() {
    test_waitpid_basic();
    test_waitpid_wnohang();
    test_waitpid_status_null();
    test_waitpid_no_children();
    test_waitpid_multiple_sequential();
    return 0;
}
