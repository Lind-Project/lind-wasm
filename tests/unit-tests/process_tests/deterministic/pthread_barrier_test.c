#include <assert.h>
#include <pthread.h>
#include <stdio.h>

pthread_barrier_t barrier;

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    printf("thread %d: before barrier\n", id);
    int ret = pthread_barrier_wait(&barrier);
    printf("thread %d: after barrier (ret=%d)\n", id, ret);
    return NULL;
}

int main(void) {
    int ids[2] = {1, 2};
    pthread_t t1, t2;

    pthread_barrier_init(&barrier, NULL, 2);
    pthread_create(&t1, NULL, thread_fn, &ids[0]);
    pthread_create(&t2, NULL, thread_fn, &ids[1]);
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);
    pthread_barrier_destroy(&barrier);

    printf("done\n");
    return 0;
}
