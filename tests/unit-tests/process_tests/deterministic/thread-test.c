#include <pthread.h>
#include <stdio.h>

// Thread function
void* myThreadFun(void* arg) {
    printf("Hello from the thread!\n");
    return NULL;
}

int main() {
    pthread_t thread_id;

    // Create a thread that will run the myThreadFun function
    pthread_create(&thread_id, NULL, myThreadFun, NULL);

    // Wait for the thread to finish
    pthread_join(thread_id, NULL);

    printf("Thread has finished execution\n");

    return 0;
}

