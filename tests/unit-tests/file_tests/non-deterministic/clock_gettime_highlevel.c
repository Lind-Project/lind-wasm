#include <stdio.h>
#include <unistd.h>
#include <time.h>

int main() {
    struct timespec begin, end, diff;

    // Get the start time
    clock_gettime(CLOCK_REALTIME, &begin);

    printf("Sleeping for 2 seconds...\n");
    sleep(2);

    // Get the end time
    clock_gettime(CLOCK_REALTIME, &end);

    // Calculate elapsed time directly using start and end times
    if ((end.tv_nsec - begin.tv_nsec) < 0) {
        diff.tv_sec = end.tv_sec - begin.tv_sec - 1;
        diff.tv_nsec = 1000000000 + end.tv_nsec - begin.tv_nsec;
    } else {
        diff.tv_sec = end.tv_sec - begin.tv_sec;
        diff.tv_nsec = end.tv_nsec - begin.tv_nsec;
    }

    // Display results
    printf("\nStart time: %lld.%09ld seconds\n", (long long)begin.tv_sec, begin.tv_nsec);
    printf("End time: %lld.%09ld seconds\n", (long long)end.tv_sec, end.tv_nsec);
    printf("Elapsed time: %lld.%09ld seconds\n", (long long)diff.tv_sec, diff.tv_nsec);

    return 0;
}
