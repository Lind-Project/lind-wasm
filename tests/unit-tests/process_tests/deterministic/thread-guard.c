#include <pthread.h>
#include <stdio.h>
#include <unistd.h>
#include <assert.h>
#include <stdbool.h>

bool thread_run = false;

void* thread_func(void* arg) {
    thread_run = true;
    return NULL;
}

int main() {
    pthread_t thread;
    pthread_attr_t attr;

    // Initialize attributes
    pthread_attr_init(&attr);

    // Get system page size
    long page_size = sysconf(_SC_PAGESIZE);
    if (page_size == -1) {
        perror("sysconf");
        return 1;
    }

    // Set guard size to one page
    pthread_attr_setguardsize(&attr, (size_t)page_size);

    // (Optional) read it back to confirm
    size_t guard_size = 0;
    pthread_attr_getguardsize(&attr, &guard_size);

    assert(page_size == guard_size);

    // Create thread with attributes
    if (pthread_create(&thread, &attr, thread_func, NULL) != 0) {
        perror("pthread_create");
        return 1;
    }

    // Wait for thread to finish
    pthread_join(thread, NULL);

    assert(thread_run);
    // Clean up
    pthread_attr_destroy(&attr);

    return 0;
}
