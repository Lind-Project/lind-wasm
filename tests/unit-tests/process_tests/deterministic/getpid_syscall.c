#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>

void test_getpid_basic() {
    printf("[TEST 1] getpid in parent\n");
    pid_t pid = getpid();
    printf("[PARENT] getpid returned=%d\n", (int)pid);
}

void test_getpid_in_child() {
    printf("\n[TEST 2] getpid in child\n");
    pid_t pid = fork();
    if (pid == 0) {
        int child_pid = getpid();
        printf("[CHILD] getpid returned=%d\n", (int)child_pid);
        _exit(0);
    } else if (pid > 0) {
        int status;
        waitpid(pid, &status, 0);
        printf("[PARENT] child exited, parent pid=%d\n", (int)getpid());
    } else {
        printf("[ERROR] fork failed\n");
    }
}

void test_getpid_multiple_children() {
    printf("\n[TEST 3] getpid with multiple children\n");
    pid_t pids[3];
    for (int i = 0; i < 3; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            printf("[CHILD %d] getpid=%d\n", i, (int)getpid());
            _exit(10 + i);
        }
    }
    for (int i = 0; i < 3; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d with exit=%d\n", (int)pids[i], status);
    }
}

void test_getpid_nested_forks() {
    printf("\n[TEST 4] nested forks\n");
    pid_t pid = fork();
    if (pid == 0) {
        printf("[CHILD] getpid=%d\n", (int)getpid());
        pid_t grandchild = fork();
        if (grandchild == 0) {
            printf("[GRANDCHILD] getpid=%d\n", (int)getpid());
            _exit(0);
        } else if (grandchild > 0) {
            int status;
            waitpid(grandchild, &status, 0);
            _exit(0);
        }
    } else if (pid > 0) {
        int status;
        waitpid(pid, &status, 0);
    }
}

void test_getpid_stress() {
    printf("\n[TEST 5] stress test with 20 children\n");
    const int N = 20;
    pid_t pids[N];
    for (int i = 0; i < N; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            printf("[CHILD %d] getpid=%d\n", i, (int)getpid());
            _exit(i); // different exit code
        }
    }
    for (int i = 0; i < N; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], status);
    }
}

int main() {
    printf("[RUNNING] getpid test suite\n");
    test_getpid_basic();
    test_getpid_in_child();
    test_getpid_multiple_children();
    test_getpid_nested_forks();
    test_getpid_stress();
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
