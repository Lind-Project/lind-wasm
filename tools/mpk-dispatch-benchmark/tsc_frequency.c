#define _GNU_SOURCE
#include <cpuid.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>

static uint64_t ticks(void) {
    unsigned lo, hi;
    __asm__ volatile("lfence; rdtsc; lfence"
                     : "=a"(lo), "=d"(hi) : : "memory");
    return ((uint64_t)hi << 32) | lo;
}

static uint64_t ns(struct timespec t) {
    return (uint64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}

int main(void) {
    unsigned a, b, c, d;
    double cpuid_hz = 0;
    if (__get_cpuid_count(0x15, 0, &a, &b, &c, &d) && a && b && c)
        cpuid_hz = (double)c * b / a;
    printf("{\"cpuid_hz\":%.0f,\"calibrated_hz\":[", cpuid_hz);
    for (int i = 0; i < 7; ++i) {
        struct timespec start, end, delay = {.tv_nsec = 200000000};
        if (clock_gettime(CLOCK_MONOTONIC_RAW, &start) != 0) {
            perror("clock_gettime");
            return 1;
        }
        uint64_t t0 = ticks();
        if (nanosleep(&delay, NULL) != 0) {
            perror("nanosleep");
            return 1;
        }
        uint64_t t1 = ticks();
        if (clock_gettime(CLOCK_MONOTONIC_RAW, &end) != 0) {
            perror("clock_gettime");
            return 1;
        }
        printf("%s%.0f", i ? "," : "", (double)(t1 - t0) * 1e9 / (ns(end) - ns(start)));
    }
    puts("]}");
    return 0;
}
