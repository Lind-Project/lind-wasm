#include <stdio.h>
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <time.h>

#define LOOP_COUNT 1000000

long long gettimens() {
    struct timespec tp;
    clock_gettime(CLOCK_MONOTONIC, &tp);
    return (long long)tp.tv_sec * 1000000000LL + tp.tv_nsec;
}

unsigned long long fibonacci(int n) {
    if (n <= 1) return n;
    volatile  unsigned long long a = 0, b = 1, c;
    for (int i = 2; i <= n; i++) {
        c = a + b;
        a = b;
        b = c;
    }
    return b;
}

int main() {
    unsigned long long sum = 0;

    long long start_time = gettimens();

    for (int i = 0; i < LOOP_COUNT; i++) {
        sum += fibonacci(1000); 
    }
    
    long long end_time = gettimens();
    long long total_time = (end_time - start_time) / 1000000;
    fprintf(stderr, "start: %lld\n", start_time);
    fprintf(stderr, "end: %lld\n", end_time);
    fprintf(stderr, "total: %lld\n", total_time);
    fflush(stderr);

    return 0;
}