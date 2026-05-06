/*
 * SIGUSR1 + MAP_SHARED coordination test.
 *
 * Mirrors PostgreSQL's ProcSignalBarrier pattern:
 *   - parent and child share an mmap'd page (MAP_ANONYMOUS | MAP_SHARED).
 *   - child writes a sentinel into the shared region, then sends SIGUSR1
 *     to the parent.
 *   - parent's SIGUSR1 handler sets a flag.  The parent's main loop waits
 *     up to 5s for the flag, then reads the sentinel back from shmem.
 *
 * Failure modes this catches:
 *   - kill(parent, SIGUSR1) is dropped or misrouted between cages.
 *   - the shared mmap region is not actually shared after fork (parent
 *     reads zero / its own private copy).
 *
 * Passes natively and under any runtime that correctly supports
 * cross-process signal delivery + shared anonymous memory.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t got_sigusr1 = 0;

static void sigusr1_handler(int signo) {
    (void)signo;
    got_sigusr1 = 1;
}

#define SENTINEL 0xC0FFEE42u

int main(void) {
    /* Install handler BEFORE fork so child inherits it (we don't actually
     * need it in the child, but installing first removes a race). */
    struct sigaction sa = {0};
    sa.sa_handler = sigusr1_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_RESTART;
    if (sigaction(SIGUSR1, &sa, NULL) < 0) {
        perror("sigaction"); return 1;
    }

    /* Tiny shared region — a single u32. */
    size_t pagesize = 4096;
    void *region = mmap(NULL, pagesize, PROT_READ | PROT_WRITE,
                        MAP_ANONYMOUS | MAP_SHARED, -1, 0);
    if (region == MAP_FAILED) {
        perror("mmap"); return 1;
    }
    volatile unsigned int *cell = (volatile unsigned int *)region;
    *cell = 0;

    pid_t parent_pid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }

    if (pid == 0) {
        /* Child: write sentinel, signal parent. */
        *cell = SENTINEL;
        /* Memory barrier so the write is visible before the signal. */
        __sync_synchronize();

        if (kill(parent_pid, SIGUSR1) < 0) {
            perror("child: kill");
            _exit(1);
        }
        _exit(0);
    }

    /* Parent: wait up to 5s for SIGUSR1, then check the shared region. */
    struct timespec start, now;
    clock_gettime(CLOCK_MONOTONIC, &start);
    while (!got_sigusr1) {
        clock_gettime(CLOCK_MONOTONIC, &now);
        double elapsed = (now.tv_sec - start.tv_sec)
                       + (now.tv_nsec - start.tv_nsec) / 1e9;
        if (elapsed > 5.0) {
            fprintf(stderr,
                    "[parent] FAIL: SIGUSR1 not delivered after 5s "
                    "(*cell = 0x%x, expected 0x%x)\n",
                    *cell, SENTINEL);
            return 1;
        }
        /* tight wait — pause(2) would race with already-delivered signal */
        struct timespec ts = { .tv_sec = 0, .tv_nsec = 10 * 1000 * 1000 };
        nanosleep(&ts, NULL);
    }

    if (*cell != SENTINEL) {
        fprintf(stderr,
                "[parent] FAIL: signal delivered, but shmem cell = 0x%x "
                "(expected 0x%x — MAP_SHARED not actually shared)\n",
                *cell, SENTINEL);
        return 1;
    }

    int status = 0;
    waitpid(pid, &status, 0);
    int child_rc = WIFEXITED(status) ? WEXITSTATUS(status) : 1;
    if (child_rc != 0) {
        fprintf(stderr, "[parent] FAIL: child exited %d\n", child_rc);
        return 1;
    }

    fprintf(stderr, "[parent] PASS\n");
    munmap(region, pagesize);
    return 0;
}
