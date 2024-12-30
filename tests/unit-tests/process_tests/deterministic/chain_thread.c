#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

// Function for the second-level thread
void* inner_thread_function(void* arg) {
    printf("Inner thread running...\n");
    sleep(1); // Simulate work
    printf("Inner thread done.\n");
    return NULL;
}

// Function for the first-level thread
void* outer_thread_function(void* arg) {
    printf("Outer thread running...\n");
    
    // Create the inner thread
    pthread_t inner_thread;
    if (pthread_create(&inner_thread, NULL, inner_thread_function, NULL) != 0) {
        perror("Failed to create inner thread");
        return NULL;
    }

    // Wait for the inner thread to finish
    if (pthread_join(inner_thread, NULL) != 0) {
        perror("Failed to join inner thread");
        return NULL;
    }

    printf("Outer thread done.\n");
    return NULL;
}

int main() {
    pthread_t outer_thread;

    // Create the outer thread
    if (pthread_create(&outer_thread, NULL, outer_thread_function, NULL) != 0) {
        perror("Failed to create outer thread");
        exit(1);
    }

    // Wait for the outer thread to finish
    if (pthread_join(outer_thread, NULL) != 0) {
        perror("Failed to join outer thread");
        exit(1);
    }

    printf("Main thread done.\n");
    return 0;
}
