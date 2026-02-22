/* Test that flockfile/funlockfile work correctly under contention.
 *
 * Two threads synchronize at a barrier so they race into flockfile(stdout)
 * simultaneously.  Inside the critical section each thread does fwrite+fflush.
 * The loser must be woken by funlockfile's futex_wake.
 *
 * Root cause of the original hang: _IO_lock_unlock took a single-threaded
 * fast path (plain store, no futex_wake) because SINGLE_THREAD_P was 1.
 */
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>

pthread_barrier_t barrier;

void *thread_fn(void *arg) {
    int id = *(int *)arg;

    pthread_barrier_wait(&barrier);

    flockfile(stdout);
    char buf[64];
    int n = snprintf(buf, sizeof(buf), "thread %d: hello\n", id);
    fwrite(buf, 1, n, stdout);
    fflush(stdout);
    funlockfile(stdout);

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
