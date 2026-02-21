/* Test: threads synchronize with barrier, then exit.
 * Reproduces the pattern from pthread_barrier_test that was hanging.
 */
#include <stdio.h>
#include <pthread.h>

pthread_barrier_t barrier;

static void *thread_fn(void *arg) {
    int id = *(int *)arg;
    printf("thread %d: before barrier\n", id);
    pthread_barrier_wait(&barrier);
    printf("thread %d: after barrier\n", id);
    return NULL;
}

int main(void) {
    pthread_t t1, t2;
    int ids[2] = {1, 2};

    pthread_barrier_init(&barrier, NULL, 2);
    pthread_create(&t1, NULL, thread_fn, &ids[0]);
    pthread_create(&t2, NULL, thread_fn, &ids[1]);
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);
    pthread_barrier_destroy(&barrier);
    printf("done\n");
    return 0;
}
