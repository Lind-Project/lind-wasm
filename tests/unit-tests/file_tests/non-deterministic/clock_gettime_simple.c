#include <time.h>
#include <stdio.h>

int main() {
    struct timespec tp;

    // get CLOCK_REALTIME's time
    if (clock_gettime(CLOCK_REALTIME, &tp) == -1) {
        perror("clock_gettime failed");
        return 1;
    }

    //print the result
    printf("Current time: %lld seconds and %ld nanoseconds\n", tp.tv_sec, tp.tv_nsec);

    return 0;
}



