// DESCRIPTION: Measures cage metadata lookup and fork/wait lifecycle latency.
/*
 * Microbenchmark for process/cage metadata lookup and fork lifecycle overhead.
 *
 * Each result uses benchrunner.py's tab-delimited output format:
 *
 *   <test>\t<param>\t<loops>\t<average nanoseconds>
 *
 * The lookup workloads exercise getpid(), getppid(), getuid(), and geteuid().
 * The fork workload also exercises cage allocation, parent/child bookkeeping,
 * child exit, zombie recording, SIGCHLD delivery, waitpid(), and cleanup.
 */
#define _GNU_SOURCE
#include "bench.h"

#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define LOOKUP_LOOPS LOOPS_LARGE
#define FORK_LOOPS 2000
#define THREAD_COUNT 4
#define WARMUP_LOOPS LOOPS_SMALL

static long long bench_getpid_loop(int loops) {
    volatile pid_t sink = 0;
    long long start = gettimens();

    for (int i = 0; i < loops; i++) {
        sink ^= getpid();
    }

    long long end = gettimens();

    if (sink == (pid_t)-1) {
        fprintf(stderr, "impossible getpid sink value\n");
    }

    return (end - start) / loops;
}

static long long bench_getppid_loop(int loops) {
    volatile pid_t sink = 0;
    long long start = gettimens();

    for (int i = 0; i < loops; i++) {
        sink ^= getppid();
    }

    long long end = gettimens();

    if (sink == (pid_t)-1) {
        fprintf(stderr, "impossible getppid sink value\n");
    }

    return (end - start) / loops;
}

static long long bench_mixed_lookup_loop(int loops) {
    volatile long sink = 0;
    long long start = gettimens();

    for (int i = 0; i < loops; i++) {
        switch (i & 3) {
            case 0:
                sink ^= getpid();
                break;
            case 1:
                sink ^= getppid();
                break;
            case 2:
                sink ^= getuid();
                break;
            case 3:
                sink ^= geteuid();
                break;
        }
    }

    long long end = gettimens();

    if (sink == -1) {
        fprintf(stderr, "impossible mixed lookup sink value\n");
    }

    return (end - start) / loops;
}

static long long bench_fork_wait(int loops) {
    long long start = gettimens();

    for (int i = 0; i < loops; i++) {
        pid_t pid = fork();

        if (pid < 0) {
            perror("fork");
            fprintf(stderr, "failed at fork iteration %d\n", i);
            exit(1);
        }

        if (pid == 0) {
            _exit(i & 0xff);
        }

        int status = 0;
        pid_t waited = waitpid(pid, &status, 0);
        if (waited < 0) {
            perror("waitpid");
            exit(1);
        }

        if (!WIFEXITED(status)) {
            fprintf(stderr, "child did not exit normally at iteration %d\n", i);
            exit(1);
        }

        if (WEXITSTATUS(status) != (i & 0xff)) {
            fprintf(stderr,
                    "bad exit status at iteration %d: got %d expected %d\n",
                    i,
                    WEXITSTATUS(status),
                    i & 0xff);
            exit(1);
        }
    }

    long long end = gettimens();
    return (end - start) / loops;
}

struct thread_arg {
    int loops;
    int tid;
};

static void *thread_lookup_worker(void *argp) {
    struct thread_arg *arg = argp;
    volatile long sink = arg->tid;

    for (int i = 0; i < arg->loops; i++) {
        switch ((i + arg->tid) & 3) {
            case 0:
                sink ^= getpid();
                break;
            case 1:
                sink ^= getppid();
                break;
            case 2:
                sink ^= getuid();
                break;
            case 3:
                sink ^= geteuid();
                break;
        }
    }

    return NULL;
}

static long long bench_threaded_lookup(int threads, int loops_per_thread) {
    pthread_t *thread_ids = calloc((size_t)threads, sizeof(*thread_ids));
    struct thread_arg *args = calloc((size_t)threads, sizeof(*args));

    if (thread_ids == NULL || args == NULL) {
        perror("calloc");
        exit(1);
    }

    long long start = gettimens();

    for (int i = 0; i < threads; i++) {
        args[i].loops = loops_per_thread;
        args[i].tid = i;

        int rc = pthread_create(&thread_ids[i], NULL, thread_lookup_worker, &args[i]);
        if (rc != 0) {
            errno = rc;
            perror("pthread_create");
            exit(1);
        }
    }

    for (int i = 0; i < threads; i++) {
        int rc = pthread_join(thread_ids[i], NULL);
        if (rc != 0) {
            errno = rc;
            perror("pthread_join");
            exit(1);
        }
    }

    long long end = gettimens();

    free(thread_ids);
    free(args);

    return (end - start) / (threads * loops_per_thread);
}

int main(void) {
    const int threaded_loops = LOOKUP_LOOPS / THREAD_COUNT;
    const int threaded_total_ops = threaded_loops * THREAD_COUNT;

    /* Reduce one-time initialization noise without emitting warmup rows. */
    (void)bench_getpid_loop(WARMUP_LOOPS);
    (void)bench_getppid_loop(WARMUP_LOOPS);
    (void)bench_mixed_lookup_loop(WARMUP_LOOPS);

    emit_result_string("Cage getpid",
                       "-",
                       bench_getpid_loop(LOOKUP_LOOPS),
                       LOOKUP_LOOPS);
    emit_result_string("Cage getppid",
                       "-",
                       bench_getppid_loop(LOOKUP_LOOPS),
                       LOOKUP_LOOPS);
    emit_result_string("Cage mixed lookup",
                       "-",
                       bench_mixed_lookup_loop(LOOKUP_LOOPS),
                       LOOKUP_LOOPS);
    emit_result("Cage threaded lookup",
                THREAD_COUNT,
                bench_threaded_lookup(THREAD_COUNT, threaded_loops),
                threaded_total_ops);
    emit_result_string("Cage fork + waitpid",
                       "-",
                       bench_fork_wait(FORK_LOOPS),
                       FORK_LOOPS);

    return 0;
}
