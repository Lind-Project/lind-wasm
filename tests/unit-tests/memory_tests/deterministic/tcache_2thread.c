#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define NALLOCS 32
#define SIZES_COUNT 4
static const size_t sizes[SIZES_COUNT] = {16, 32, 64, 128};

static void tag(int id, int phase) {
    char buf[] = "[T0:P0]\n";
    buf[2] = '0' + id;
    buf[5] = '0' + phase;
    write(2, buf, 8);
}

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    void *ptrs[NALLOCS];
    tag(id, 0);
    tag(id, 1);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) { write(2, "malloc failed\n", 14); return (void *)1; }
        memset(ptrs[i], id + i, sz);
    }
    tag(id, 2);
    for (int i = 0; i < NALLOCS; i++) free(ptrs[i]);
    tag(id, 3);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) { write(2, "re-malloc failed\n", 17); return (void *)1; }
        memset(ptrs[i], 0xAA, sz);
    }
    tag(id, 4);
    for (int i = 0; i < NALLOCS; i++) free(ptrs[i]);
    tag(id, 5);
    return NULL;
}

int main(void) {
    int ids[2] = {1, 2};
    pthread_t threads[2];
    for (int i = 0; i < 2; i++)
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    int failed = 0;
    for (int i = 0; i < 2; i++) {
        void *ret;
        pthread_join(threads[i], &ret);
        if (ret != NULL) failed = 1;
    }
    if (failed) { write(1, "FAIL\n", 5); return 1; }
    write(1, "done\n", 5);
    return 0;
}
