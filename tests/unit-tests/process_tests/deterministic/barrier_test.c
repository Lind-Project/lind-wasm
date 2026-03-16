/* Test that pthread_barrier_wait and pthread_join work correctly.
 *
 * Two threads synchronize at a barrier, then exit. Main joins both.
 *
 * Without fixes:
 *   - exit(0) in thread exit path runs _IO_cleanup, which tries to lock
 *     all FILEs and deadlocks if another thread holds a stdio lock.
 *   - pd->tid = 0 (plain store) may not be visible to pthread_join's
 *     atomic_load_acquire under WASM's weak memory model.
 *
 * With fixes: threads use _exit(0) and atomic_store_release(&pd->tid, 0).
 */
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>

pthread_barrier_t barrier;
volatile int thread_passed[2] = {0, 0};

void *thread_fn(void *arg) {
    int id = *(int *)arg;

    int ret = pthread_barrier_wait(&barrier);
    if (ret != 0 && ret != PTHREAD_BARRIER_SERIAL_THREAD) {
        _exit(1);
    }
    thread_passed[id] = 1;

    return NULL;
}

int main(void) {
    int ids[2] = {0, 1};
    pthread_t t1, t2;

    pthread_barrier_init(&barrier, NULL, 2);
    pthread_create(&t1, NULL, thread_fn, &ids[0]);
    pthread_create(&t2, NULL, thread_fn, &ids[1]);
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);
    pthread_barrier_destroy(&barrier);

    if (!thread_passed[0] || !thread_passed[1]) {
        write(2, "thread did not pass barrier\n", 27);
        return 1;
    }

    write(1, "done\n", 5);
    return 0;
}
