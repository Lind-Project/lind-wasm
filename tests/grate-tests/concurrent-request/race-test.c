/* race_test.c — Test binary to trigger the get_cage() race condition.
 *
 * Strategy: fork N children, each spawning M threads that do work involving
 * mmap (thread stack allocation) and futex (pthread sync). Children exit
 * quickly, creating concurrent remove_cage() + get_cage() calls.
 *
 * The race window:
 *   - Child's last thread calls exit_syscall → remove_cage()
 *   - Meanwhile, sibling children or their threads call mmap/futex
 *   - get_cage() returns None → unwrap() panics in signal.rs / fs_calls.rs
 *
 * Run with the race-test grate:
 *   lind-wasm race_test_grate.cwasm race_test.cwasm
 *
 * Without the runtime fix, this should panic intermittently.
 */
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <unistd.h>
#include <sys/wait.h>

#define NUM_FORKS   5
#define NUM_THREADS 10
#define ITERATIONS  500

static volatile int sink;

static void *thread_fn(void *arg) {
    /* Each iteration touches mmap (stack), futex (pthread internals),
     * and generates general syscall traffic. */
    for (int i = 0; i < ITERATIONS; i++)
        sink = i;
    return NULL;
}

static void child_work(void) {
    pthread_t threads[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++)
        pthread_create(&threads[i], NULL, thread_fn, NULL);
    for (int i = 0; i < NUM_THREADS; i++)
        pthread_join(threads[i], NULL);

    /* Child exits immediately after threads finish — triggers remove_cage()
     * while siblings may still be in mmap/futex handlers. */
}

int main(void) {
    pid_t pids[NUM_FORKS];

    for (int i = 0; i < NUM_FORKS; i++) {
        pids[i] = fork();
        if (pids[i] < 0) {
            perror("fork");
            exit(1);
        }
        if (pids[i] == 0) {
            child_work();
            exit(0);
        }
    }

    /* Wait for all children */
    for (int i = 0; i < NUM_FORKS; i++)
        waitpid(pids[i], NULL, 0);

    printf("pass\n");
    return 0;
}
