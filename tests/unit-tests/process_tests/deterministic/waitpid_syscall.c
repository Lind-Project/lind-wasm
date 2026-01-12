#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>

void test_waitpid_basic() {
    printf("[TEST 1] Basic waitpid\n");

    pid_t pid = fork();
    if (pid == 0) {
        _exit(1);
    }

    int status;
    waitpid(pid, &status, 0);
    printf("[PARENT] child exited\n");
}

void test_waitpid_wnohang() {
    printf("\n[TEST 2] waitpid WNOHANG\n");

    pid_t pid = fork();
    if (pid == 0) {
        sleep(1);
        _exit(0);
    }

    int status;
    waitpid(pid, &status, WNOHANG);  // ignore return value

    printf("[PARENT] WNOHANG invoked\n");

    waitpid(pid, &status, 0);
    printf("[PARENT] child exited\n");
}

void test_waitpid_status_null() {
    printf("\n[TEST 3] waitpid status NULL\n");

    pid_t pid = fork();
    if (pid == 0) {
        _exit(0);
    }

    waitpid(pid, NULL, 0);
    printf("[PARENT] child exited\n");
}

void test_waitpid_no_children() {
    printf("\n[TEST 4] waitpid no children\n");

    int status;
    pid_t r = waitpid(-1, &status, WNOHANG);
    printf("[PARENT] waitpid returned=%d\n", (int)r);
}

void test_waitpid_multiple_sequential() {
    printf("\n[TEST 5] Sequential children\n");

    for (int i = 0; i < 3; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            _exit(0);
        }
        waitpid(pid, NULL, 0);
        printf("[PARENT] reaped child %d\n", i);
    }
}

int main() {
    printf("[RUNNING] waitpid test suite\n");

    test_waitpid_basic();
    test_waitpid_wnohang();
    test_waitpid_status_null();
    test_waitpid_no_children();
    test_waitpid_multiple_sequential();

    printf("\n[DONE]\n");
    return 0;
}
