// DESCRIPTION: Evaluate kernel-resolved syscall latency using geteuid().
#include "bench.h"
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
	int ret;

	long long start = gettimens();
	for (int i = 0; i < LOOPS_LARGE; i++) {
		ret = geteuid();
	}
	long long end = gettimens();
	long long avg = (end - start) / LOOPS_LARGE;

	emit_result_string("geteuid", "-", avg, LOOPS_LARGE);
}
