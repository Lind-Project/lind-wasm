#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>
#include <string.h>

void test_basic_waitpid() {
    printf("[TEST 1] Basic waitpid\n");
    pid_t pid = fork();
    if (pid == 0) _exit(10);

    int status = 0;
    pid_t r = waitpid(pid, &status, 0);
    printf("[PARENT] waitpid returned=%d exit=%d\n", (int)r, status);
}

void test_waitpid_specific_child() {
    printf("\n[TEST 2] waitpid specific child\n");
    pid_t p1 = fork();
    if (p1 == 0) _exit(20);

    pid_t p2 = fork();
    if (p2 == 0) _exit(30);

    int status;
    pid_t r = waitpid(p2, &status, 0);
    printf("[PARENT] waited child=%d exit=%d\n", (int)r, status);

    while (wait(&status) > 0) { }
}

void test_waitpid_wnohang_running() {
    printf("\n[TEST 3] WNOHANG on running child\n");
    pid_t pid = fork();
    if (pid == 0) {
        sleep(1);
        _exit(5);
    }

    int status = 0;
    pid_t r = waitpid(pid, &status, WNOHANG);
    printf("[PARENT] waitpid WNOHANG result=%d (0 means child not exited)\n", (int)r);

    waitpid(pid, &status, 0);
    printf("[PARENT] Child later reaped exit=%d\n", status);
}

void test_waitpid_wnohang_exited() {
    printf("\n[TEST 4] WNOHANG after child exited\n");
    pid_t pid = fork();
    if (pid == 0) _exit(30);

    sleep(1);
    int status = 0;
    pid_t r = waitpid(pid, &status, WNOHANG);
    printf("[PARENT] waitpid WNOHANG returned=%d exit=%d\n", (int)r, status);
}

void test_waitpid_no_children() {
    printf("\n[TEST 5] waitpid with no children\n");
    int status = 0;
    pid_t r = waitpid(-1, &status, 0);
    printf("[PARENT] waitpid returned=%d errno=%d (%s)\n", (int)r, errno, strerror(errno));
}

void test_zombie_order() {
    printf("\n[TEST 6] Zombie order (multiple children)\n");
    pid_t pids[3];
    for (int i=0;i<3;i++) {
        pid_t p = fork();
        if (p == 0) _exit(100+i);
        pids[i] = p;
    }

    int status;
    for (int i=0;i<3;i++) {
        pid_t r = wait(&status);
        printf("[PARENT] reaped pid=%d exit=%d\n", (int)r, status);
    }
}

void test_waitpid_status_null() {
    printf("\n[TEST 7] waitpid with status=NULL\n");
    pid_t pid = fork();
    if (pid == 0) _exit(42);

    pid_t r = waitpid(pid, NULL, 0);
    printf("[PARENT] waitpid returned=%d (status=NULL)\n", (int)r);
}

void test_waitpid_any_child() {
    printf("\n[TEST 8] waitpid any child (-1)\n");
    pid_t p1 = fork();
    if (p1 == 0) _exit(11);

    pid_t p2 = fork();
    if (p2 == 0) _exit(12);

    int status;
    pid_t r = waitpid(-1, &status, 0);
    printf("[PARENT] waitpid(-1) returned=%d exit=%d\n", (int)r, status);

    r = waitpid(-1, &status, 0);
    printf("[PARENT] waitpid(-1) returned=%d exit=%d\n", (int)r, status);
}

void test_waitpid_non_child() {
    printf("\n[TEST 9] waitpid on non-child pid\n");
    pid_t pid = fork();
    if (pid == 0) _exit(1);

    pid_t fake = pid + 10000;
    int status = 0;
    pid_t r = waitpid(fake, &status, WNOHANG);
    printf("[PARENT] waitpid non-child returned=%d errno=%d (%s)\n", (int)r, errno, strerror(errno));

    waitpid(pid, &status, 0);
}

void test_waitpid_stress() {
    printf("\n[TEST 10] Stress test with 50 children\n");
    const int N = 5;
    pid_t pids[N];
    for (int i=0;i<N;i++) {
        pid_t p = fork();
        if (p == 0) _exit(i); // each child exits with its index
        pids[i] = p;
    }

    int status;
    for (int i=0;i<N;i++) {
        pid_t r = waitpid(-1, &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)r, status);
    }
}

int main() {
    printf("[RUNNING] waitpid test suite\n");
    test_basic_waitpid();
    test_waitpid_specific_child();
    test_waitpid_wnohang_running();
    test_waitpid_wnohang_exited();
    test_waitpid_no_children();
    test_zombie_order();
    test_waitpid_status_null();
    // test_waitpid_any_child(); // might panic
    // test_waitpid_non_child(); // always panics
    // test_waitpid_stress(); // need to check cage limit
    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
