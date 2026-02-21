/* Test: pthread_create must not clobber __lind_cageid in shared memory.
 *
 * Before the clone3.c fix, the child thread's clone3 return path would
 * write __lind_cageid = 0 to shared linear memory before re-querying
 * the host.  This raced with the parent (and sibling) threads, which
 * read __lind_cageid during every syscall.  A zero cage ID causes a
 * 3i panic: "handler table for cage 0 not found".
 *
 * This test spawns enough threads to reliably trigger the race window.
 * With the fix, it should print "done" and exit cleanly every time.
 */
#include <stdio.h>
#include <pthread.h>

#define NUM_THREADS 20

static volatile int dummy;

static void *thread_fn(void *arg) {
    for (int i = 0; i < 1000; i++)
        dummy = i;
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
