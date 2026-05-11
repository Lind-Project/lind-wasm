/*
 * thread_dlopen_concurrent.c
 *
 * Tests Task 3 (issue #1028): dlopen called from one thread while other threads
 * are already running.
 *
 * Scenario:
 *   1. Spawn NUM_WORKERS threads.  They block on g_ready, signalling to main
 *      that they are live and registered in the epoch system.
 *   2. Main waits at g_ready (ensuring all workers are registered), then calls
 *      dlopen().  epoch_dlopen_trigger_others fires EPOCH_DLOPEN on every
 *      registered worker thread.
 *   3. Main releases workers via g_go.  The epoch check at pthread_barrier_wait's
 *      function entry delivers the pending EPOCH_DLOPEN (handle_dlopen_replay),
 *      so lib.cwasm is loaded into each worker's store before any library call.
 *   4. Each worker calls the dlopen'd function via a function pointer that was
 *      passed from the main thread, AND independently via dlsym().  Both paths
 *      must succeed for the test to pass.
 *   5. Main thread joins all workers and verifies exit codes.
 *
 * Two-barrier design:
 *   g_ready (count = NUM_WORKERS + 1):
 *     Workers call this first so main can confirm they are all running
 *     (registered) before invoking dlopen.  Without this guarantee,
 *     epoch_dlopen_trigger_others might fire before some threads are visible.
 *
 *   g_go (count = NUM_WORKERS + 1):
 *     Main calls this after dlopen.  The epoch check at pthread_barrier_wait's
 *     Wasm function entry processes EPOCH_DLOPEN (handle_dlopen_replay) before
 *     workers execute any indirect call into the library.
 */

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <dlfcn.h>

#define NUM_WORKERS 4

typedef int (*add_fn)(int, int);

/* Shared state written by main after dlopen, read by workers after g_go. */
static void  *g_handle = NULL;
static add_fn g_add_fp = NULL;

static pthread_barrier_t g_ready; /* workers → main: "I am alive and registered" */
static pthread_barrier_t g_go;    /* main → workers: "dlopen is done, proceed" */

static void *worker(void *arg)
{
    long id = (long)arg;

    /* Signal to main that this thread is alive and registered in the epoch
     * system.  Main will not call dlopen until all workers reach here. */
    pthread_barrier_wait(&g_ready);

    /* Wait for main to complete dlopen.
     * The epoch check at pthread_barrier_wait's Wasm function entry delivers
     * any pending EPOCH_DLOPEN (handle_dlopen_replay), ensuring lib.cwasm is
     * installed into this thread's store before we call any library symbol. */
    pthread_barrier_wait(&g_go);

    /* ---- call via inherited function pointer ---- */
    int result1 = g_add_fp(3, 4);
    if (result1 != 7) {
        fprintf(stderr, "worker %ld: add via fp gave %d, expected 7\n", id, result1);
        return (void *)1L;
    }

    /* ---- call via independent dlsym in this thread ---- */
    dlerror();
    add_fn fn2 = (add_fn)dlsym(g_handle, "add");
    char *err = dlerror();
    if (err) {
        fprintf(stderr, "worker %ld: dlsym failed: %s\n", id, err);
        return (void *)2L;
    }
    if (!fn2) {
        fprintf(stderr, "worker %ld: dlsym returned NULL\n", id);
        return (void *)3L;
    }

    int result2 = fn2(10, 20);
    if (result2 != 30) {
        fprintf(stderr, "worker %ld: add via dlsym gave %d, expected 30\n", id, result2);
        return (void *)4L;
    }

    return (void *)0L;
}

int main(void)
{
    if (pthread_barrier_init(&g_ready, NULL, NUM_WORKERS + 1) != 0 ||
        pthread_barrier_init(&g_go,    NULL, NUM_WORKERS + 1) != 0) {
        fprintf(stderr, "barrier_init failed\n");
        return 1;
    }

    /* Spawn workers before dlopen so they are live when dlopen fires. */
    pthread_t tids[NUM_WORKERS];
    for (long i = 0; i < NUM_WORKERS; i++) {
        if (pthread_create(&tids[i], NULL, worker, (void *)i) != 0) {
            fprintf(stderr, "pthread_create failed for worker %ld\n", i);
            return 1;
        }
    }

    /* Wait until all workers have reached g_ready, guaranteeing they are
     * registered in the epoch system before we call dlopen. */
    pthread_barrier_wait(&g_ready);

    /* dlopen while all workers are alive and registered.
     * epoch_dlopen_trigger_others sets EPOCH_DLOPEN on every worker. */
    g_handle = dlopen("lib.cwasm", RTLD_LAZY | RTLD_GLOBAL);
    if (!g_handle) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }

    dlerror();
    g_add_fp = (add_fn)dlsym(g_handle, "add");
    char *err = dlerror();
    if (err || !g_add_fp) {
        fprintf(stderr, "dlsym(add) failed: %s\n", err ? err : "(null symbol)");
        dlclose(g_handle);
        return 1;
    }

    /* Confirm the library works in the main thread before releasing workers. */
    int main_result = g_add_fp(1, 2);
    if (main_result != 3) {
        fprintf(stderr, "main: add gave %d, expected 3\n", main_result);
        dlclose(g_handle);
        return 1;
    }

    /* Release workers.  The epoch check at pthread_barrier_wait's entry in
     * each worker processes EPOCH_DLOPEN before any library call. */
    pthread_barrier_wait(&g_go);

    int failed = 0;
    for (int i = 0; i < NUM_WORKERS; i++) {
        void *ret;
        pthread_join(tids[i], &ret);
        if ((long)ret != 0) {
            fprintf(stderr, "worker %d reported error %ld\n", i, (long)ret);
            failed = 1;
        }
    }

    dlclose(g_handle);
    pthread_barrier_destroy(&g_ready);
    pthread_barrier_destroy(&g_go);

    if (failed) {
        fprintf(stderr, "FAIL\n");
        return 1;
    }
    printf("OK\n");
    return 0;
}
