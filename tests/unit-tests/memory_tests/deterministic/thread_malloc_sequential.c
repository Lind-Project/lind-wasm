/* Threads run fully sequentially — thread 1 finishes before thread 2 starts.
   Eliminates ALL concurrency. If this crashes, the bug is in heap state
   corruption from multiple threads' tcache, not concurrent access. */
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

    tag(id, 1);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) { write(2, "malloc failed\n", 14); return (void *)1; }
        memset(ptrs[i], id + i, sz);
    }

    tag(id, 2);
    for (int i = 0; i < NALLOCS; i++)
        free(ptrs[i]);

    tag(id, 3);
    for (int i = 0; i < NALLOCS; i++) {
        size_t sz = sizes[i % SIZES_COUNT];
        ptrs[i] = malloc(sz);
        if (!ptrs[i]) { write(2, "re-malloc failed\n", 17); return (void *)1; }
        memset(ptrs[i], 0xAA, sz);
    }

    tag(id, 4);
    for (int i = 0; i < NALLOCS; i++)
        free(ptrs[i]);

    tag(id, 5);
    return NULL;
}

int main(void) {
    /* Run threads one at a time — fully sequential */
    for (int i = 0; i < 4; i++) {
        int id = i + 1;
        pthread_t t;
        pthread_create(&t, NULL, thread_fn, &id);
        void *ret;
        pthread_join(t, &ret);
        if (ret != NULL) {
            write(1, "FAIL\n", 5);
            return 1;
        }
    }
    write(1, "done\n", 5);
    return 0;
}
