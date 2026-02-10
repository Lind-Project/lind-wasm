#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>

__thread int tls_var = 233;

#define NUM_THREADS 5

void* thread_function(void* arg) {
    int thread_id = *((int*)arg);
    assert(tls_var == 233);
    tls_var = thread_id * 10;
    assert(tls_var == thread_id * 10);
    return NULL;
}

int main() {
    pthread_t threads[NUM_THREADS];
    int thread_ids[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++) {
        thread_ids[i] = i + 1;
        pthread_create(&threads[i], NULL, thread_function, &thread_ids[i]);
    }

    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    return 0;
}
