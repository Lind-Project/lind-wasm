/* thread_race.c — Cage-side test with a global cap on total calls.
 *
 * Spawns NUM_THREADS threads. Together they perform at most TOTAL_CALLS
 * geteuid() calls in total. This is useful when the runtime can only
 * support a bounded number of calls in one test run.
 *
 * Pair with: thread_race_grate.c
 * Run: lind-wasm thread_race_grate.cwasm thread_race.cwasm
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

    printf("[thread_race] PASS: %d threads x %d calls returned 10\n",
           NUM_THREADS, CALLS_PER_THREAD);
    return 0;
}
