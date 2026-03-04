#include "bench.h"
#include <time.h>
#include <stdio.h>

// Returns a monotonic timestamp in nanoseconds.
long long gettimens() {
	struct timespec tp;
	clock_gettime(CLOCK_MONOTONIC, &tp);
	return (long long)tp.tv_sec * 1000000000LL + tp.tv_nsec;
}

// Emits one benchmark row in the format:
// <test>\t<param>\t<loops>\t<avg_ns>
void emit_result(char *test, int param, long long average, int loops) {
	printf("%s\t%d\t%d\t%lld\n", test, param, loops, average);
}
