/*
 * waitpid() Test Suite
 * --------------------
 * Tests various waitpid() scenarios and process reaping behavior.
 * Covers:
 *   - Basic waitpid() and waiting for specific child
 *   - WNOHANG with running or exited children
 *   - waitpid() with no children or non-child PIDs
 *   - Zombie processes and multiple children
 *   - waitpid() with status=NULL
 *   - Stress test with multiple children
 *   - EINTR signal handling during waitpid()
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <errno.h>
#include <string.h>
#include <signal.h>

void handler(int sig) { }


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

    while (wait(&status) > 0) {}
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
    for (int i=0; i<3; i++) {
        pid_t p = fork();
        if (p == 0) _exit(100+i);
        pids[i] = p;
    }

    int status;
    for (int i=0; i<3; i++) {
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

void test_waitpid_non_child() {
    printf("\n[TEST 8] waitpid on non-child pid\n");
    pid_t pid = fork();
    if (pid == 0) _exit(1);

    pid_t fake = pid + 10000;
    int status = 0;
    pid_t r = waitpid(fake, &status, WNOHANG);
    printf("[PARENT] waitpid non-child returned=%d errno=%d (%s)\n",
           (int)r, errno, strerror(errno));

    waitpid(pid, &status, 0);
}

void test_waitpid_stress() {
    printf("\n[TEST 9] Stress test with 5 children\n");
    const int N = 5;
    pid_t pids[N];
    for (int i=0; i<N; i++) {
        pid_t p = fork();
        if (p == 0) _exit(i);
        pids[i] = p;
    }

    int status;
    for (int i=0; i<N; i++) {
        pid_t r = waitpid(-1, &status, 0);
        printf("[PARENT] reaped child %d exit=%d\n", (int)r, status);
    }
}

void test_waitpid_eintr() {
    printf("\n[TEST 10] waitpid EINTR\n");

    signal(SIGUSR1, handler);

    pid_t pid = fork();
    if (pid == 0) {
        sleep(2);
        _exit(99);
    }

    kill(getpid(), SIGUSR1);

    int status = 0;
    pid_t r = waitpid(pid, &status, 0);

    printf("[PARENT] waitpid returned=%d errno=%d (%s)\n",
           (int)r, errno, strerror(errno));
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
    // test_waitpid_non_child();
    // test_waitpid_stress();
    test_waitpid_eintr();

    printf("\n[ALL TESTS COMPLETED]\n");
    return 0;
}
