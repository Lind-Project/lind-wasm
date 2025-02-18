#include <stdio.h>
#include <time.h>

/*
    This test indirectly calls the clock_gettime syscall through the clock() function.
    The clock() function in glibc internally uses clock_gettime to measure CPU time 
    consumed by the process.

    The program measures the CPU time taken to run 1,000,000 iterations of a simple loop.
    It records the start time using clock(), performs the computation, then records the
    end time and calculates the elapsed CPU time.
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

