/* Test that printf works from multiple threads without deadlocking.
 *
 * Two threads both call printf on stdout concurrently.
 * Without fixes: _IO_lock_unlock skips futex_wake when SINGLE_THREAD_P
 * is true, so a thread waiting on the stdout lock sleeps forever.
 */
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    printf("hello from thread %d\n", id);
    return NULL;
}

int main(void) {
    int ids[2] = {1, 2};
    pthread_t t1, t2;
    pthread_create(&t1, NULL, thread_fn, &ids[0]);
    pthread_create(&t2, NULL, thread_fn, &ids[1]);
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);
    write(1, "done\n", 5);
    return 0;
}
