// DESCRIPTION: CPU-only Fibonacci(1000) loop to measure compute overhead.
#include "bench.h"

#define LOOP_COUNT 10000
#define FIB_INPUT 1000

unsigned long long fibonacci(int n) {
	if (n <= 1)
		return n;
	volatile unsigned long long a = 0, b = 1, c;
	for (int i = 2; i <= n; i++) {
		c = a + b;
		a = b;
		b = c;
	}
	return b;
}

int main() {
	volatile unsigned long long sum = 0;

	long long start_time = gettimens();
	for (int i = 0; i < LOOP_COUNT; i++) {
		sum += fibonacci(FIB_INPUT);
	}
	long long end_time = gettimens();

	long long avg_time = (end_time - start_time) / LOOP_COUNT;

	emit_result("Fibonacci", 0, avg_time, LOOP_COUNT);

	return 0;
}
