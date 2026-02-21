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

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    char buf[64];
    int len;

    len = snprintf(buf, sizeof(buf), "thread %d: before barrier\n", id);
    write(1, buf, len);

    int ret = pthread_barrier_wait(&barrier);

    len = snprintf(buf, sizeof(buf), "thread %d: past barrier (ret=%d)\n", id, ret);
    write(1, buf, len);

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
