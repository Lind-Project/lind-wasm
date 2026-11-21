#include "bench.h"
#include <time.h>
#include <stdio.h>

// Shared sizes for FS and IPC read/writes.
int fs_sizes[4] = {1, KiB(1), KiB(4), KiB(10)};
int ipc_sizes[4] = {1, KiB(1), KiB(4), KiB(10)};

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

// Emits benchmark row with a string param.
void emit_result_string(char *test, char *param, long long average, int loops) {
	printf("%s\t%s\t%d\t%lld\n", test, param, loops, average);
}
