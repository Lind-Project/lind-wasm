/*
 * Microbenchmark for process/cage metadata lookup and fork lifecycle overhead.
 *
 * This benchmark measures the cost of several lightweight process-related
 * syscalls that are expected to exercise the runtime's hot-path cage lookup
 * logic, including getpid(), getppid(), getuid(), and geteuid().
 *
 * It also measures fork() + waitpid() performance, which exercises the heavier
 * cage lifecycle path: cage ID allocation, child cage creation, parent/child
 * bookkeeping, child exit, zombie recording, SIGCHLD delivery, waitpid()
 * wakeup, fd-table cleanup, and final cage removal.
 *
 * The benchmark reports total operations, elapsed time, nanoseconds per
 * operation, and operations per second for each workload. A short warmup phase
 * is run before the measured section to reduce one-time initialization noise.
 *
 * Usage:
 *   ./benchmark [iters] [forks] [threads]
 *
 * Defaults:
 *   iters   = 1000000
 *   forks   = 2000
 *   threads = 4
 */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#ifndef DEFAULT_ITERS
#define DEFAULT_ITERS 1000000
#endif

#ifndef DEFAULT_FORKS
#define DEFAULT_FORKS 2000
#endif

#ifndef DEFAULT_THREADS
#define DEFAULT_THREADS 4
#endif

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        perror("clock_gettime");
        exit(1);
    }
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static void print_result(const char *name, uint64_t ops, uint64_t ns) {
    double sec = (double)ns / 1e9;
    double ns_per_op = ops ? (double)ns / (double)ops : 0.0;
    double ops_per_sec = sec > 0.0 ? (double)ops / sec : 0.0;

    printf("%-28s ops=%" PRIu64 " time=%.6f sec ns/op=%.2f ops/sec=%.2f\n",
           name, ops, sec, ns_per_op, ops_per_sec);
}

static void bench_getpid_loop(uint64_t iters) {
    volatile pid_t sink = 0;

    uint64_t start = now_ns();

    for (uint64_t i = 0; i < iters; i++) {
        sink ^= getpid();
    }

    uint64_t end = now_ns();

    if (sink == (pid_t)-1) {
        fprintf(stderr, "impossible sink value\n");
    }

    print_result("getpid loop", iters, end - start);
}

static void bench_getppid_loop(uint64_t iters) {
    volatile pid_t sink = 0;

    uint64_t start = now_ns();

    for (uint64_t i = 0; i < iters; i++) {
        sink ^= getppid();
    }

    uint64_t end = now_ns();

    if (sink == (pid_t)-1) {
        fprintf(stderr, "impossible sink value\n");
    }

    print_result("getppid loop", iters, end - start);
}

static void bench_mixed_lookup_loop(uint64_t iters) {
    volatile long sink = 0;

    uint64_t start = now_ns();

    for (uint64_t i = 0; i < iters; i++) {
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

    uint64_t end = now_ns();

    if (sink == -1) {
        fprintf(stderr, "impossible sink value\n");
    }

    print_result("mixed lookup syscalls", iters, end - start);
}

static void bench_fork_wait(uint64_t forks) {
    uint64_t start = now_ns();

    for (uint64_t i = 0; i < forks; i++) {
        pid_t pid = fork();

        if (pid < 0) {
            perror("fork");
            fprintf(stderr, "failed at fork iteration %" PRIu64 "\n", i);
            exit(1);
        }

        if (pid == 0) {
            _exit((int)(i & 0xff));
        }

        int status = 0;
        pid_t waited = waitpid(pid, &status, 0);
        if (waited < 0) {
            perror("waitpid");
            exit(1);
        }

        if (!WIFEXITED(status)) {
            fprintf(stderr, "child did not exit normally at iteration %" PRIu64 "\n", i);
            exit(1);
        }

        if (WEXITSTATUS(status) != (int)(i & 0xff)) {
            fprintf(stderr,
                    "bad exit status at iteration %" PRIu64 ": got %d expected %d\n",
                    i,
                    WEXITSTATUS(status),
                    (int)(i & 0xff));
            exit(1);
        }
    }

    uint64_t end = now_ns();

    print_result("fork + waitpid", forks, end - start);
}

struct thread_arg {
    uint64_t iters;
    int tid;
};

static void *thread_lookup_worker(void *argp) {
    struct thread_arg *arg = (struct thread_arg *)argp;
    volatile long sink = arg->tid;

    for (uint64_t i = 0; i < arg->iters; i++) {
        switch ((i + (uint64_t)arg->tid) & 3) {
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

    return (void *)(uintptr_t)(sink & 0xff);
}

static void bench_threaded_lookup(int threads, uint64_t iters_per_thread) {
    pthread_t *ths = calloc((size_t)threads, sizeof(pthread_t));
    struct thread_arg *args = calloc((size_t)threads, sizeof(struct thread_arg));

    if (!ths || !args) {
        perror("calloc");
        exit(1);
    }

    uint64_t start = now_ns();

    for (int i = 0; i < threads; i++) {
        args[i].iters = iters_per_thread;
        args[i].tid = i;

        int rc = pthread_create(&ths[i], NULL, thread_lookup_worker, &args[i]);
        if (rc != 0) {
            errno = rc;
            perror("pthread_create");
            exit(1);
        }
    }

    for (int i = 0; i < threads; i++) {
        void *ret = NULL;
        int rc = pthread_join(ths[i], &ret);
        if (rc != 0) {
            errno = rc;
            perror("pthread_join");
            exit(1);
        }
    }

    uint64_t end = now_ns();

    uint64_t total_ops = (uint64_t)threads * iters_per_thread;
    print_result("pthread mixed lookup", total_ops, end - start);

    free(ths);
    free(args);
}

static void usage(const char *prog) {
    fprintf(stderr,
            "usage: %s [iters] [forks] [threads]\n"
            "\n"
            "defaults:\n"
            "  iters   = %d\n"
            "  forks   = %d\n"
            "  threads = %d\n"
            "\n"
            "examples:\n"
            "  %s\n"
            "  %s 10000000 5000 8\n",
            prog,
            DEFAULT_ITERS,
            DEFAULT_FORKS,
            DEFAULT_THREADS,
            prog,
            prog);
}

int main(int argc, char **argv) {
    uint64_t iters = DEFAULT_ITERS;
    uint64_t forks = DEFAULT_FORKS;
    int threads = DEFAULT_THREADS;

    if (argc > 4) {
        usage(argv[0]);
        return 2;
    }

    if (argc >= 2) {
        iters = strtoull(argv[1], NULL, 10);
    }

    if (argc >= 3) {
        forks = strtoull(argv[2], NULL, 10);
    }

    if (argc >= 4) {
        threads = atoi(argv[3]);
        if (threads <= 0) {
            fprintf(stderr, "threads must be positive\n");
            return 2;
        }
    }

    printf("pid=%ld iters=%" PRIu64 " forks=%" PRIu64 " threads=%d\n",
           (long)getpid(), iters, forks, threads);

    /*
     * Warmup. This reduces one-time initialization noise from the benchmark.
     */
    bench_getpid_loop(10000);
    bench_getppid_loop(10000);
    bench_mixed_lookup_loop(10000);

    printf("\n--- measured ---\n");

    bench_getpid_loop(iters);
    bench_getppid_loop(iters);
    bench_mixed_lookup_loop(iters);
    bench_threaded_lookup(threads, iters / (uint64_t)threads);
    bench_fork_wait(forks);

    return 0;
}
