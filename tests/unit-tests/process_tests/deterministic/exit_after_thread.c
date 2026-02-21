/* Test: create one thread, join it, then return from main.
 * If this hangs but exit_return_main doesn't, the exit hang is
 * specific to programs that have used pthreads.
 */
#include <stdio.h>
#include <pthread.h>

static void *thread_fn(void *arg) {
    (void)arg;
    printf("child thread done\n");
    return NULL;
}

int main(void) {
    pthread_t t;
    pthread_create(&t, NULL, thread_fn, NULL);
    pthread_join(t, NULL);
    printf("main done\n");
    return 0;
}
