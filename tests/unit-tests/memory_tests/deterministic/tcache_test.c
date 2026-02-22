#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/*
 * Test that malloc's tcache (thread-local cache) works correctly.
 * Tcache caches recently freed small chunks per-thread for fast reuse.
 * This test:
 *   1. Allocates and frees small chunks (triggers tcache fill)
 *   2. Re-allocates same sizes (should hit tcache)
 *   3. Does this from multiple threads (each thread has its own tcache)
 *   4. Verifies memory contents are correct (no corruption)
 */

#define NALLOCS 32
#define SIZES_COUNT 4

static const size_t sizes[SIZES_COUNT] = {16, 32, 64, 128};

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    void *ptrs[NALLOCS];

    /* Phase 1: alloc + write */
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) {
            write(2, "malloc failed\n", 14);
            return (void *)1;
        }
        memset(ptrs[i], id + i, sz);
    }

    /* Phase 2: free all (populates tcache) */
    for (int i = 0; i < NALLOCS; i++) {
        free(ptrs[i]);
    }

    /* Phase 3: re-alloc same sizes (should hit tcache) + verify clean use */
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) {
            write(2, "re-malloc failed\n", 17);
            return (void *)1;
        }
        /* Write pattern and verify */
        memset(ptrs[i], 0xAA, sz);
        unsigned char *p = (unsigned char *)ptrs[i];
        for (size_t j = 0; j < sz; j++) {
            if (p[j] != 0xAA) {
                write(2, "corruption\n", 11);
                return (void *)1;
            }
        }
    }

    /* Cleanup */
    for (int i = 0; i < NALLOCS; i++) {
        free(ptrs[i]);
    }

    return NULL;
}

int main(void) {
    /* Single-threaded tcache test first */
    void *p1 = malloc(48);
    void *p2 = malloc(48);
    free(p1);
    free(p2);
    /* These should come from tcache (same size bin) */
    void *p3 = malloc(48);
    void *p4 = malloc(48);
    /* With tcache, freed chunks are reused LIFO */
    if (p3 == p2 && p4 == p1) {
        write(1, "tcache reuse: yes\n", 18);
    } else {
        write(1, "tcache reuse: no\n", 17);
    }
    free(p3);
    free(p4);

    /* Multi-threaded test */
    int ids[4] = {1, 2, 3, 4};
    pthread_t threads[4];
    for (int i = 0; i < 4; i++) {
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    }

    int failed = 0;
    for (int i = 0; i < 4; i++) {
        void *ret;
        pthread_join(threads[i], &ret);
        if (ret != NULL) failed = 1;
    }

    if (failed) {
        write(1, "FAIL\n", 5);
        return 1;
    }
    write(1, "done\n", 5);
    return 0;
}
