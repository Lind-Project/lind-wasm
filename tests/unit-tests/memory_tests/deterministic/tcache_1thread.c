#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define NALLOCS 32
#define SIZES_COUNT 4
static const size_t sizes[SIZES_COUNT] = {16, 32, 64, 128};

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    void *ptrs[NALLOCS];
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) { write(2, "malloc failed\n", 14); return (void *)1; }
        memset(ptrs[i], id + i, sz);
    }
    for (int i = 0; i < NALLOCS; i++) free(ptrs[i]);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) { write(2, "re-malloc failed\n", 17); return (void *)1; }
        memset(ptrs[i], 0xAA, sz);
    }
    for (int i = 0; i < NALLOCS; i++) free(ptrs[i]);
    return NULL;
}

int main(void) {
    int id = 1;
    pthread_t t;
    pthread_create(&t, NULL, thread_fn, &id);
    void *ret;
    pthread_join(t, &ret);
    if (ret != NULL) { write(1, "FAIL\n", 5); return 1; }
    write(1, "done\n", 5);
    return 0;
}
