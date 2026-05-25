#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/wait.h>

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1.0e6;
}

static void run_test(const char *label) {
    char dest[128] = {0};
    const char *src = "hello from per-cage remote-call test";

    double t0 = now_ms();
    strcpy(dest, src);
    double elapsed = now_ms() - t0;

    printf("[%s] result=\"%s\"  %.3f ms\n", label, dest, elapsed);
    fflush(stdout);
}

int main(void) {
    /*
     * Fork twice from the initial cage (cage 1) so that:
     *   cage 1 (this process)     — local strcpy, no routing
     *   cage 2 (first child)      — strcpy via Unix domain socket
     *   cage 3 (second child)     — strcpy via TCP socket
     *
     * The routing.json routes are keyed by cageid so each cage automatically
     * picks the right policy without any in-process branching logic.
     */
    run_test("cage1/no-interpose ");

    pid_t pid1 = fork();
    if (pid1 < 0) { perror("fork1"); return 1; }
    if (pid1 == 0) {
        run_test("cage2/inter-process");
        return 0;
    }

    pid_t pid2 = fork();
    if (pid2 < 0) { perror("fork2"); return 1; }
    if (pid2 == 0) {
        run_test("cage3/inter-machine");
        return 0;
    }

    waitpid(pid1, NULL, 0);
    waitpid(pid2, NULL, 0);
    return 0;
}
