/* Test that flockfile/funlockfile work correctly under contention.
 *
 * Two threads synchronize at a barrier so they race into flockfile
 * simultaneously.  Inside the critical section each thread does fwrite+fflush.
 * The loser must be woken by funlockfile's futex_wake.
 *
 * Root cause of the original hang: _IO_lock_unlock took a single-threaded
 * fast path (plain store, no futex_wake) because SINGLE_THREAD_P was 1.
 *
 * Output goes to /dev/null so test output is deterministic.
 */
#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

pthread_barrier_t barrier;
FILE *sink;

void *thread_fn(void *arg) {
    int id = *(int *)arg;

    pthread_barrier_wait(&barrier);

    flockfile(sink);
    char buf[64];
    int n = snprintf(buf, sizeof(buf), "thread %d: hello\n", id);
    assert(fwrite(buf, 1, n, sink) == (size_t)n);
    assert(fflush(sink) == 0);
    funlockfile(sink);

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
