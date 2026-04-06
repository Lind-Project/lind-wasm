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

#define NUM_THREADS 20
#define TOTAL_CALLS 5000

static int next_call = 0;
static pthread_mutex_t call_mu = PTHREAD_MUTEX_INITIALIZER;

static void *thread_fn(void *arg) {
    int tid = (int)(long)arg;

    while (1) {
        int my_call;

        if (pthread_mutex_lock(&call_mu) != 0) {
            fprintf(stderr, "[thread %d] FAIL: pthread_mutex_lock failed\n", tid);
            assert(0);
        }

        if (next_call >= TOTAL_CALLS) {
            pthread_mutex_unlock(&call_mu);
            break;
        }

        my_call = next_call;
        next_call++;

        if (pthread_mutex_unlock(&call_mu) != 0) {
            fprintf(stderr, "[thread %d] FAIL: pthread_mutex_unlock failed\n", tid);
            assert(0);
        }

        int ret = geteuid();
        if (ret != 10) {
            fprintf(stderr,
                    "[thread %d] FAIL: global_call %d, expected 10, got %d\n",
                    tid, my_call, ret);
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

    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    printf("[thread_race] PASS: %d total calls across %d threads returned 10\n",
           TOTAL_CALLS, NUM_THREADS);
    return 0;
}
