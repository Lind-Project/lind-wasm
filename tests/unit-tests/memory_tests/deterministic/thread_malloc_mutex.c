/* Same as tcache_test but wraps malloc/free in an explicit mutex.
   If this never crashes, the glibc arena lock (lll_lock) is broken. */
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define NALLOCS 32
#define SIZES_COUNT 4
static const size_t sizes[SIZES_COUNT] = {16, 32, 64, 128};

static pthread_mutex_t global_lock = PTHREAD_MUTEX_INITIALIZER;

static void tag(int id, int phase) {
    char buf[] = "[T0:P0]\n";
    buf[2] = '0' + id;
    buf[5] = '0' + phase;
    write(2, buf, 8);
}

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    void *ptrs[NALLOCS];

    tag(id, 1);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        pthread_mutex_lock(&global_lock);
        ptrs[i] = malloc(sz);
        pthread_mutex_unlock(&global_lock);
        if (!ptrs[i]) { write(2, "malloc failed\n", 14); return (void *)1; }
        memset(ptrs[i], id + i, sz);
    }

    tag(id, 2);
    for (int i = 0; i < NALLOCS; i++) {
        pthread_mutex_lock(&global_lock);
        free(ptrs[i]);
        pthread_mutex_unlock(&global_lock);
    }

    tag(id, 3);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        pthread_mutex_lock(&global_lock);
        ptrs[i] = malloc(sz);
        pthread_mutex_unlock(&global_lock);
        if (!ptrs[i]) { write(2, "re-malloc failed\n", 17); return (void *)1; }
        memset(ptrs[i], 0xAA, sz);
    }

    tag(id, 4);
    for (int i = 0; i < NALLOCS; i++) {
        pthread_mutex_lock(&global_lock);
        free(ptrs[i]);
        pthread_mutex_unlock(&global_lock);
    }

    tag(id, 5);
    return NULL;
}

int main(void) {
    int ids[4] = {1, 2, 3, 4};
    pthread_t threads[4];
    for (int i = 0; i < 4; i++)
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    int failed = 0;
    for (int i = 0; i < 4; i++) {
        void *ret;
        pthread_join(threads[i], &ret);
        if (ret != NULL) failed = 1;
    }
    if (failed) { write(1, "FAIL\n", 5); return 1; }
    write(1, "done\n", 5);
    return 0;
}
