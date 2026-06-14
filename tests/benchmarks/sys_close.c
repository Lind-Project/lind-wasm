// DESCRIPTION: Evaluate no-op syscall latency using close(-1).
#include "bench.h"
#include <unistd.h>

int main() {
	long long start_time = gettimens();
	for (int i = 0; i < LOOPS_LARGE; i++) {
		close(-1);
	}
	long long end_time = gettimens();
	long long average_time = (end_time - start_time) / LOOPS_LARGE;

	emit_result("close", -1, average_time, LOOPS_LARGE);
}
