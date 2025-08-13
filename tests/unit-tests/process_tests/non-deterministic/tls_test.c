#include <stdio.h>
#include <pthread.h>
#include <unistd.h>

// Define thread-local variable (TLS)
__thread int tls_var = 233;  // Each thread will have its own instance

#define NUM_THREADS 5

// Function executed by each thread
void* thread_function(void* arg) {
    int thread_id = *((int*)arg);
    printf("Thread %d: initial tls_var(%u) = %d\n", thread_id, &tls_var, tls_var);
    tls_var = thread_id * 10;  // Modify the thread-local variable
    sleep(1);  // Simulate work
    printf("Thread %d (after sleep): tls_var(%u) = %d\n", thread_id, &tls_var, tls_var);
    return NULL;
}

int main() {
    pthread_t threads[NUM_THREADS];
    int thread_ids[NUM_THREADS];

    // Create threads
    for (int i = 0; i < NUM_THREADS; i++) {
        thread_ids[i] = i + 1;
        pthread_create(&threads[i], NULL, thread_function, &thread_ids[i]);
    }

    // Wait for threads to complete
    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    return 0;
}
