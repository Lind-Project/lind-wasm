#include "bench.h"
#include <unistd.h>

#define LOOP_COUNT 1000000

int main() {
	long long start_time = gettimens();
	for (int i = 0; i < LOOP_COUNT; i++) {
		close(-1);
	}
	long long end_time = gettimens();
	long long average_time = (end_time - start_time) / LOOP_COUNT;

	emit_result("close", -1, average_time, LOOP_COUNT);
}
