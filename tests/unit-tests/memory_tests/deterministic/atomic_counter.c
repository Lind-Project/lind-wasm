/* Direct test: do WASM atomics work across threads?
   4 threads each increment a shared counter 1000 times via CAS.
   Expected final value: 4000.  If atomics are broken, value < 4000. */
#include <pthread.h>
#include <unistd.h>

static volatile int counter = 0;  /* shared across all threads */

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    for (int i = 0; i < 1000; i++) {
        /* manual CAS loop — same primitive as lll_lock */
        while (1) {
            int old = __sync_val_compare_and_swap(&counter, counter, counter);
            if (__sync_bool_compare_and_swap(&counter, old, old + 1))
                break;
        }
    }
    char buf[] = "[T0:done]\n";
    buf[2] = '0' + id;
    write(2, buf, 10);
    return NULL;
}

int main(void) {
    int ids[4] = {1, 2, 3, 4};
    pthread_t threads[4];
    for (int i = 0; i < 4; i++)
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    for (int i = 0; i < 4; i++)
        pthread_join(threads[i], NULL);

    /* Print final counter value */
    int val = counter;
    char buf[32];
    int n = 0;
    const char *msg = "counter=";
    while (*msg) buf[n++] = *msg++;
    if (val >= 1000) buf[n++] = '0' + (val / 1000) % 10;
    if (val >= 100)  buf[n++] = '0' + (val / 100) % 10;
    if (val >= 10)   buf[n++] = '0' + (val / 10) % 10;
    buf[n++] = '0' + val % 10;

    if (val == 4000) {
        const char *ok = " OK\n";
        while (*ok) buf[n++] = *ok++;
        write(1, buf, n);
        return 0;
    } else {
        const char *bad = " WRONG (expected 4000)\n";
        while (*bad) buf[n++] = *bad++;
        write(1, buf, n);
        return 1;
    }
}
