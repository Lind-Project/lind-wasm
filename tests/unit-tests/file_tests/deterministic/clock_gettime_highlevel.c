#include <assert.h>
#include <stdint.h>
#include <time.h>

#define LARGE 100000000ULL

int main(void) {
    clock_t start = clock();
    assert(start != (clock_t)-1);

    volatile uint64_t x = 0;
    for (uint64_t i = 0; i < LARGE; i++)
        x += i;

    clock_t end = clock();
    assert(end != (clock_t)-1);
    assert(end >= start);

    clock_t elapsed_ticks = end - start;
    assert(elapsed_ticks >= 0);

    return 0;
}
