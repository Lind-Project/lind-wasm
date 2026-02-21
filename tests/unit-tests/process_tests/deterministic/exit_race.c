/* Test: maximize the race between child thread cleanup and main thread exit.
 *
 * Race condition: after pthread_join returns (child set pd->tid=0 + futex_wake),
 * the child thread is still alive in start_thread cleanup and hasn't called
 * exit(0) yet. If the main thread's exit reaches lind_thread_exit while the
 * child's epoch_handler is still registered, lind_thread_exit returns false
 * (not last thread) and lind_manager.decrement() is never called, causing
 * lind_manager.wait() to block forever.
 *
 * Many threads with no work maximizes the probability of hitting this race.
 */
#include <stdio.h>
#include <pthread.h>

#define NUM_THREADS 20

static void *thread_fn(void *arg) {
    (void)arg;
    return NULL;
}

int main(void) {
    pthread_t threads[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++)
        pthread_create(&threads[i], NULL, thread_fn, NULL);
    for (int i = 0; i < NUM_THREADS; i++)
        pthread_join(threads[i], NULL);

    printf("done\n");
    return 0;
}
