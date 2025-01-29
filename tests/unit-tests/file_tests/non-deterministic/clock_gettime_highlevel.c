#include <stdio.h>
#include <time.h>

/*
    This is higher level test for clock_gettime, which we don't directly call clock_gettime function but still call this syscall eventually
*/

int main() {
    // Get the start time using clock()
    clock_t begin = clock();

    printf("Running 1,000,000 iterations...\n");

    volatile long long sum = 0;
    for (long long i = 0; i < 1000000; i++) {
        sum += i;
    }

    // Get the end time using clock()
    clock_t end = clock();

    // Calculate elapsed time in seconds
    double elapsed_time = (double)(end - begin) / CLOCKS_PER_SEC;

    // Display results
    printf("\nStart time: %lld clock ticks\n", begin);
    printf("End time: %lld clock ticks\n", end);
    printf("Elapsed CPU time: %.9f seconds\n", elapsed_time);

    return 0;
}

