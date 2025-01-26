#include <stdio.h>
#include <unistd.h>
#include <time.h>
#include <sys/time.h>

int main() {
    // Step 1: Get the current time using time()
    time_t current_time = time(NULL);
    if (current_time == ((time_t)-1)) {
        perror("time() failed");
        return 1;
    }
    printf("Current time (time): %ld seconds since the Epoch\n", current_time);

    // Step 2: Get the current time using gettimeofday()
    struct timeval tv;
    if (gettimeofday(&tv, NULL) == -1) {
        perror("gettimeofday() failed");
        return 1;
    }
    printf("Current time (gettimeofday): %ld seconds and %ld microseconds since the Epoch\n",
           tv.tv_sec, tv.tv_usec);

    // Step 3: Wait for 2 seconds using sleep() and measure the time again
    printf("Sleeping for 2 seconds...\n");
    sleep(2);

    time_t new_time = time(NULL);
    if (new_time == ((time_t)-1)) {
        perror("time() failed after sleep");
        return 1;
    }
    printf("New time (time): %ld seconds since the Epoch\n", new_time);

    // Verify that the time has increased by approximately 2 seconds
    if (new_time - current_time >= 2) {
        printf("Test Passed: Time increased as expected.\n");
    } else {
        printf("Test Failed: Time did not increase correctly.\n");
    }

    return 0;
}
