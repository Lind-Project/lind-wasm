/* store_race.c — Cage-side test for concurrent grate Store access (#961).
 *
 * Spawns NUM_THREADS threads that each call geteuid() (interposed by the
 * grate) CALLS_PER_THREAD times. This forces concurrent
 * grate_callback_trampoline invocations from different host threads into
 * the same Wasmtime Store, reproducing the Store concurrency bug.
 *
 * The grate handler does heap allocations, shared state mutation, and
 * pointer chasing to exercise the same memory patterns as a real grate
 * (e.g. fdtables/DashMap).
 *
 * Pair with: store_race_grate.c
 * Run: lind-wasm store_race_grate.cwasm store_race.cwasm
 */
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <pthread.h>
#include <assert.h>

#define NUM_THREADS      20
#define CALLS_PER_THREAD 100000

static void *thread_fn(void *arg) {
    int tid = (int)(long)arg;
    for (int i = 0; i < CALLS_PER_THREAD; i++) {
        int ret = geteuid();
        if (ret != 10) {
            fprintf(stderr, "[thread %d] FAIL: iteration %d, expected 10, got %d\n",
                    tid, i, ret);
            assert(0);
        }
    }
    return NULL;
}

int main(void) {
    pthread_t threads[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++) {
        int ret = pthread_create(&threads[i], NULL, thread_fn, (void *)(long)i);
        if (ret != 0) {
            fprintf(stderr, "pthread_create failed: %d\n", ret);
            exit(1);
        }
    }

    for (int i = 0; i < NUM_THREADS; i++)
        pthread_join(threads[i], NULL);

    printf("[store_race] PASS: %d threads x %d calls returned 10\n",
           NUM_THREADS, CALLS_PER_THREAD);
    return 0;
}
