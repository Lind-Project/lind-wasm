/*
 * SIGUSR1 + MAP_SHARED coordination between siblings (postmaster pattern).
 *
 * postmaster forks ~10 auxiliaries (startup, bgwriter, checkpointer,
 * walwriter, autovacuum launcher, ...).  ProcSignalBarrier requires any
 * one of those auxiliaries to be able to signal *any other* via SIGUSR1
 * + a shared shmem ack flag.  None of them are direct descendants of
 * each other — only of postmaster.
 *
 * This test:
 *   - parent mmaps a shared region holding two cells: child_a_pid and
 *     child_b_seen_signal.
 *   - parent forks child A and child B.
 *   - A reads B's pid from a shared cell after A is up, then kill(B, SIGUSR1).
 *   - B's SIGUSR1 handler sets the shmem ack cell.
 *   - Parent waits up to 5s and verifies the ack cell is set.
 *
 * Catches signal-delivery bugs that only manifest when the killer and
 * killee are siblings rather than parent↔child.
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

struct shared {
    volatile pid_t b_pid;          /* B publishes its pid here */
    volatile sig_atomic_t b_acked; /* B sets this in its handler */
    volatile sig_atomic_t a_done;  /* A sets this when it has signaled B */
};

static struct shared *g_shared;

static void b_handler(int signo) {
    (void)signo;
    g_shared->b_acked = 1;
}

static int run_b(void) {
    struct sigaction sa = {0};
    sa.sa_handler = b_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_RESTART;
    if (sigaction(SIGUSR1, &sa, NULL) < 0) {
        perror("B: sigaction"); return 1;
    }
    /* Publish our pid to siblings via shmem. */
    g_shared->b_pid = getpid();
    __sync_synchronize();

    /* Wait up to 5s to receive the signal. */
    struct timespec start, now;
    clock_gettime(CLOCK_MONOTONIC, &start);
    while (!g_shared->b_acked) {
        clock_gettime(CLOCK_MONOTONIC, &now);
        double elapsed = (now.tv_sec - start.tv_sec)
                       + (now.tv_nsec - start.tv_nsec) / 1e9;
        if (elapsed > 5.0) {
            fprintf(stderr, "[B] FAIL: SIGUSR1 not received after 5s\n");
            return 1;
        }
        struct timespec ts = { .tv_sec = 0, .tv_nsec = 10 * 1000 * 1000 };
        nanosleep(&ts, NULL);
    }
    return 0;
}

static int run_a(void) {
    /* Wait for B to publish its pid. */
    struct timespec start, now;
    clock_gettime(CLOCK_MONOTONIC, &start);
    while (g_shared->b_pid == 0) {
        clock_gettime(CLOCK_MONOTONIC, &now);
        double elapsed = (now.tv_sec - start.tv_sec)
                       + (now.tv_nsec - start.tv_nsec) / 1e9;
        if (elapsed > 5.0) {
            fprintf(stderr, "[A] FAIL: B's pid never appeared in shmem\n");
            return 1;
        }
        struct timespec ts = { .tv_sec = 0, .tv_nsec = 10 * 1000 * 1000 };
        nanosleep(&ts, NULL);
    }

    if (kill(g_shared->b_pid, SIGUSR1) < 0) {
        fprintf(stderr, "[A] FAIL: kill(%d, SIGUSR1) -> %s\n",
                g_shared->b_pid, strerror(errno));
        return 1;
    }
    g_shared->a_done = 1;
    return 0;
}

int main(void) {
    size_t pagesize = 4096;
    g_shared = mmap(NULL, pagesize, PROT_READ | PROT_WRITE,
                    MAP_ANONYMOUS | MAP_SHARED, -1, 0);
    if (g_shared == MAP_FAILED) { perror("mmap"); return 1; }
    memset(g_shared, 0, sizeof(*g_shared));

    pid_t a = fork();
    if (a < 0) { perror("fork A"); return 1; }
    if (a == 0) { _exit(run_a()); }

    pid_t b = fork();
    if (b < 0) { perror("fork B"); return 1; }
    if (b == 0) { _exit(run_b()); }

    int status = 0, rc = 0;
    waitpid(a, &status, 0);
    rc |= WIFEXITED(status) ? WEXITSTATUS(status) : 1;
    waitpid(b, &status, 0);
    rc |= WIFEXITED(status) ? WEXITSTATUS(status) : 1;

    if (rc == 0 && g_shared->b_acked) {
        fprintf(stderr, "[parent] PASS: sibling A signaled sibling B\n");
    } else {
        fprintf(stderr,
                "[parent] FAIL: rc=%d a_done=%d b_acked=%d b_pid=%d\n",
                rc, g_shared->a_done, g_shared->b_acked, g_shared->b_pid);
        rc = 1;
    }
    munmap(g_shared, pagesize);
    return rc;
}
