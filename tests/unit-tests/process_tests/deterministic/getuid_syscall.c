#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

void test_getuid_basic() {
    printf("[TEST 1] getuid in parent\n");
    int uid = getuid();
    printf("[PARENT] getuid returned=%d\n", uid);
}

void test_getuid_in_child() {
    printf("\n[TEST 2] getuid in child\n");
    pid_t pid = fork();
    if (pid == 0) {
        int uid = getuid();
        printf("[CHILD] getuid returned=%d\n", uid);
        _exit(0);
    }
    waitpid(pid, NULL, 0);
    printf("[PARENT] child exited, parent getuid=%d\n", getuid());
}

void test_getuid_multiple_children() {
    printf("\n[TEST 3] getuid with multiple children\n");
    const int N = 4;
    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            int uid = getuid();
            printf("[CHILD %d] getuid=%d\n", i, uid);
            _exit(i);  // deterministic exit code
        }
        waitpid(pid, NULL, 0); // serialize children
        printf("[PARENT] reaped child %d\n", i);
    }
}

void test_getuid_stress() {
    printf("\n[TEST 4] Stress test with 2 sequential children\n");
    const int N = 2;
    for (int i = 0; i < N; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            for (int j = 0; j < 10; j++) {
                int uid = getuid();
                printf("[CHILD %d] call %d getuid=%d\n", i, j, uid);
            }
            _exit(i);  // deterministic exit code
        }
        waitpid(pid, NULL, 0); // serialize children
        printf("[PARENT] reaped child %d\n", i);
    }
}

int main() {
    printf("[RUNNING] getuid test suite\n");
    test_getuid_basic();
    test_getuid_in_child();
    test_getuid_multiple_children();
    test_getuid_stress();
    return 0;
}
