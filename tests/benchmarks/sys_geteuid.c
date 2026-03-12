// DESCRIPTION: Issues geteuid() to measure kernel-resolve syscall latency.
#include "bench.h"
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#define LOOP_COUNT 1000000

int main(int argc, char *argv[]) {
	int ret;

	long long start = gettimens();
	for (int i = 0; i < LOOP_COUNT; i++) {
		ret = geteuid();
	}
	long long end = gettimens();

	long long avg = (end - start) / LOOP_COUNT;

	emit_result_string("geteuid", "-", avg, LOOP_COUNT);
}
