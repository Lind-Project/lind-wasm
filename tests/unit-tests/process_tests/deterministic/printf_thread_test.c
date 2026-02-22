/* Test that printf works correctly from multiple threads.
 *
 * Two threads synchronize at a barrier then each call printf, which
 * internally acquires the stdio lock via _IO_lock_lock / _IO_lock_unlock.
 * This exercises the same lll_lock/lll_unlock + futex_wake path as
 * flockfile but through the implicit locking in printf.
 */
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>

pthread_barrier_t barrier;

void *thread_fn(void *arg) {
    int id = *(int *)arg;

    pthread_barrier_wait(&barrier);

    printf("thread %d: hello\n", id);

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

    write(1, "done\n", 5);
    return 0;
}
