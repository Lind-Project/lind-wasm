#include "bench.h"

#define LOOP_COUNT 10000

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
		sum += fibonacci(1000);
	}
	long long end_time = gettimens();

	long long avg_time = (end_time - start_time) / LOOP_COUNT;

	emit_result("Fibonacci", 0, avg_time, LOOP_COUNT);

	return 0;
}
