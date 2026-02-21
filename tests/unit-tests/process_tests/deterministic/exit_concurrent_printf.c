/* Test: multiple threads doing printf concurrently.
 * Tests if concurrent stdio + thread exit causes the hang.
 */
#include <stdio.h>
#include <pthread.h>

static void *thread_fn(void *arg) {
    int id = *(int *)arg;
    for (int i = 0; i < 10; i++) {
        printf("thread %d: iteration %d\n", id, i);
    }
    return NULL;
}

int main(void) {
    pthread_t threads[4];
    int ids[4] = {0, 1, 2, 3};

    for (int i = 0; i < 4; i++)
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    for (int i = 0; i < 4; i++)
        pthread_join(threads[i], NULL);

    printf("done\n");
    return 0;
}
