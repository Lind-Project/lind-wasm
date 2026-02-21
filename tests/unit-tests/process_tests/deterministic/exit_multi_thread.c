/* Test: create multiple threads, join all, then return from main.
 * Tests whether multiple thread create/exit cycles break the exit path.
 */
#include <stdio.h>
#include <pthread.h>

#define NUM_THREADS 4

static void *thread_fn(void *arg) {
    int id = *(int *)arg;
    printf("thread %d done\n", id);
    return NULL;
}

int main(void) {
    pthread_t threads[NUM_THREADS];
    int ids[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++) {
        ids[i] = i;
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    }
    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }
    printf("all threads joined\n");
    return 0;
}
