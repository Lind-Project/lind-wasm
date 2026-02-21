/* Check that TLS (__tls_base) is unique per thread.
 *
 * If &tls_var is the same address for different threads, then
 * THREAD_SELF (= &__wasilibc_pthread_self) is also the same,
 * which breaks _IO_lock_lock's recursive-owner check.
 */
#include <pthread.h>
#include <unistd.h>
#include <string.h>
#include <stdio.h>

static __thread int tls_var;

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    char buf[128];
    int len = snprintf(buf, sizeof(buf),
        "thread %d: &tls_var = %p, pthread_self = %lu\n",
        id, (void *)&tls_var, (unsigned long)pthread_self());
    write(1, buf, len);
    return NULL;
}

int main(void) {
    int ids[3] = {0, 1, 2};
    pthread_t t1, t2;

    char buf[128];
    int len = snprintf(buf, sizeof(buf),
        "main:     &tls_var = %p, pthread_self = %lu\n",
        (void *)&tls_var, (unsigned long)pthread_self());
    write(1, buf, len);

    pthread_create(&t1, NULL, thread_fn, &ids[1]);
    pthread_create(&t2, NULL, thread_fn, &ids[2]);
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);

    write(1, "done\n", 5);
    return 0;
}
