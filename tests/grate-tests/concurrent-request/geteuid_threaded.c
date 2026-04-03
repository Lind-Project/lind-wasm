/* geteuid_threaded.c — Cage-side test for concurrent grate calls.
 *
 * Spawns NUM_THREADS threads that each call geteuid() CALLS_PER_THREAD
 * times in a tight loop. Since geteuid is interposed by the grate,
 * this generates concurrent grate_callback_trampoline invocations
 * from different host threads into the same Wasmtime Store.
 *
 * This reproduces the Store concurrency bug (#961): multiple threads
 * calling TypedFunc::call() on the same Store races on StoreData's
 * internal Vecs, corrupting Wasmtime state.
 *
 * Pair with: geteuid_threaded_grate.c
 * Run: lind-wasm geteuid_threaded_grate.cwasm geteuid_threaded.cwasm
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

    printf("[Cage | geteuid] PASS: %d threads x %d calls returned 10\n",
           NUM_THREADS, CALLS_PER_THREAD);
    return 0;
}
