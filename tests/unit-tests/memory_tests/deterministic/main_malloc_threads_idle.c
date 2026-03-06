/* Main thread does all malloc/free; child threads just exist and wait.
   If this crashes, child thread stacks corrupt the heap (or vice versa). */
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile int go = 0;
static volatile int done = 0;

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    char buf[] = "[T0:wait]\n";
    buf[2] = '0' + id;
    write(2, buf, 10);

    /* spin until main says go */
    while (!go) ;

    buf[4] = 'd'; buf[5] = 'o'; buf[6] = 'n'; buf[7] = 'e';
    buf[8] = ']'; buf[9] = '\n';
    write(2, buf, 10);
    return NULL;
}

int main(void) {
    int ids[4] = {1, 2, 3, 4};
    pthread_t threads[4];

    /* create 4 idle threads */
    for (int i = 0; i < 4; i++)
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);

    /* give threads time to start */
    for (volatile int d = 0; d < 100000; d++) ;

    /* main thread does all the malloc/free */
    write(2, "[main:alloc]\n", 13);
    void *ptrs[128];
    for (int round = 0; round < 4; round++) {
        for (int i = 0; i < 128; i++) {
            ptrs[i] = malloc(64);
            if (!ptrs[i]) { write(2, "malloc fail\n", 12); return 1; }
            memset(ptrs[i], 0xBB, 64);
        }
        for (int i = 0; i < 128; i++)
            free(ptrs[i]);
    }
    write(2, "[main:done]\n", 12);

    /* release threads */
    go = 1;
    for (int i = 0; i < 4; i++)
        pthread_join(threads[i], NULL);

    write(1, "done\n", 5);
    return 0;
}
