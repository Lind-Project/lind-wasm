#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

void test_getuid_basic() {
    printf("[TEST 1] getuid in parent\n");
    pid_t uid = getuid();
    printf("[PARENT] getuid returned=%d\n", (int)uid);
}

void test_getuid_in_child() {
    printf("\n[TEST 2] getuid in child\n");
    pid_t pid = fork();
    if (pid == 0) {
        int child_uid = getuid();
        printf("[CHILD] getuid returned=%d\n", (int)child_uid);
        _exit(0);
    } else if (pid > 0) {
        int status;
        waitpid(pid, &status, 0);
        printf("[PARENT] child exited, parent getuid=%d\n", (int)getuid());
    } else {
        printf("[ERROR] fork failed\n");
    }
}

void test_getuid_multiple_children() {
    printf("\n[TEST 3] getuid with multiple children\n");
    pid_t pids[4];
    for (int i = 0; i < 4; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            printf("[CHILD %d] getuid=%d\n", i, (int)getuid());
            _exit(10 + i);
        }
    }
    for (int i = 0; i < 4; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d with exit=%d\n", (int)pids[i], status);
    }
}

void test_getuid_stress() {
    printf("\n[TEST 4] Stress test with 20 children calling getuid\n");
    pid_t pids[20];
    for (int i = 0; i < 20; i++) {
        pids[i] = fork();
        if (pids[i] == 0) {
            for (int j = 0; j < 10; j++) {
                printf("[CHILD %d] call %d getuid=%d\n", i, j, (int)getuid());
            }
            _exit(i);
        }
    }
    for (int i = 0; i < 20; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)pids[i], status);
    }
}

int main() {
    printf("[RUNNING] getuid test suite\n");
    test_getuid_basic();
    test_getuid_in_child();
    test_getuid_multiple_children();
    test_getuid_stress();
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
