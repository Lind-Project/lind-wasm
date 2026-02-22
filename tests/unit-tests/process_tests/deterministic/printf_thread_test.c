/* Test that printf works correctly from multiple threads.
 *
 * Two threads synchronize at a barrier then each call printf, which
 * internally acquires the stdio lock via _IO_lock_lock / _IO_lock_unlock.
 * This exercises the same lll_lock/lll_unlock + futex_wake path as
 * flockfile but through the implicit locking in printf.
 *
 * Printf output goes to /dev/null so test output is deterministic.
 */
#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>

pthread_barrier_t barrier;
FILE *sink;

void *thread_fn(void *arg) {
    int id = *(int *)arg;

    pthread_barrier_wait(&barrier);

    int ret = fprintf(sink, "thread %d: hello\n", id);
    assert(ret > 0);

    return NULL;
}

int main(void) {
    sink = fopen("/dev/null", "w");
    assert(sink != NULL);

    int ids[2] = {1, 2};
    pthread_t t1, t2;

    assert(pthread_barrier_init(&barrier, NULL, 2) == 0);
    assert(pthread_create(&t1, NULL, thread_fn, &ids[0]) == 0);
    assert(pthread_create(&t2, NULL, thread_fn, &ids[1]) == 0);
    assert(pthread_join(t1, NULL) == 0);
    assert(pthread_join(t2, NULL) == 0);
    pthread_barrier_destroy(&barrier);
    fclose(sink);

    write(1, "done\n", 5);
    return 0;
}
