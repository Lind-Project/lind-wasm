/*
 * thread_dlopen_concurrent.c
 *
 * Tests epoch-based cross-thread dlopen synchronization (task 3 of issue #1028).
 *
 * Scenario:
 *   - N worker threads start and block on a barrier waiting for the library.
 *   - Main thread calls dlopen(), stores the function pointer in a shared
 *     variable, then releases the barrier.
 *   - Each worker thread, after the barrier, calls the function pointer and
 *     also resolves the symbol independently via dlsym.
 *   - All calls must succeed — workers were alive BEFORE dlopen, so the
 *     runtime must have replayed the library into each thread's Wasm store
 *     via the epoch-based dlopen sync mechanism.
 */

#include <dlfcn.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

#define NUM_WORKERS 4

typedef void (*hello_fn)(const char *);

/* Shared state filled by main thread before releasing the barrier. */
static void     *g_handle     = NULL;
static hello_fn  g_hello_func = NULL;

static pthread_barrier_t g_barrier;

static void *worker(void *arg)
{
    int id = (int)(long)arg;

    /* Wait until the main thread has called dlopen(). */
    pthread_barrier_wait(&g_barrier);

    /* Call via the function pointer shared by the main thread.
       If epoch replay did not happen for this thread, the indirect call will
       trap (null or out-of-bounds call_indirect). */
    char msg[64];
    snprintf(msg, sizeof(msg), "worker %d via shared pointer", id);
    g_hello_func(msg);

    /* Re-resolve independently to confirm this thread's symbol table contains
       the library that was opened after the thread started. */
    dlerror();
    hello_fn local_fn = (hello_fn)dlsym(g_handle, "hello");
    char *err = dlerror();
    if (err) {
        fprintf(stderr, "worker %d: dlsym failed: %s\n", id, err);
        return (void *)1L;
    }

    snprintf(msg, sizeof(msg), "worker %d via own dlsym", id);
    local_fn(msg);

    return (void *)0L;
}

int main(void)
{
    pthread_t tids[NUM_WORKERS];

    pthread_barrier_init(&g_barrier, NULL, NUM_WORKERS + 1);

    /* Spawn workers — they will block on the barrier. */
    for (int i = 0; i < NUM_WORKERS; i++) {
        if (pthread_create(&tids[i], NULL, worker, (void *)(long)i) != 0) {
            fprintf(stderr, "pthread_create failed for worker %d\n", i);
            return 1;
        }
    }

    /* Open the library AFTER the threads are already running. */
    g_handle = dlopen("lib.cwasm", RTLD_LAZY);
    if (!g_handle) {
        fprintf(stderr, "dlopen failed\n");
        return 1;
    }

    dlerror();
    g_hello_func = (hello_fn)dlsym(g_handle, "hello");
    char *err = dlerror();
    if (err) {
        fprintf(stderr, "main: dlsym failed: %s\n", err);
        dlclose(g_handle);
        return 1;
    }

    /* Confirm the library works in the main thread before releasing workers. */
    g_hello_func("main thread after dlopen");

    /* Release the workers — they can now use the library. */
    pthread_barrier_wait(&g_barrier);

    /* Collect results. */
    int failed = 0;
    for (int i = 0; i < NUM_WORKERS; i++) {
        void *ret;
        pthread_join(tids[i], &ret);
        if ((long)ret != 0) {
            fprintf(stderr, "worker %d reported an error\n", i);
            failed = 1;
        }
    }

    dlclose(g_handle);
    pthread_barrier_destroy(&g_barrier);

    if (failed)
        return 1;

    printf("all workers succeeded\n");
    return 0;
}
