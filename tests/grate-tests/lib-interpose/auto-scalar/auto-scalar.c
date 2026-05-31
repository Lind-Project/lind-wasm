// Cage for auto-scalar marshalling test.
// Calls toy_add(10, 3) from libtoy.cwasm.
// The grate intercepts toy_add and returns a*b (30) instead of a+b (13).
// The cage verifies it receives 30.
#include <stdio.h>
#include <stdlib.h>

extern int toy_add(int a, int b);

int main(void) {
    int result = toy_add(10, 3);
    if (result != 30) {
        fprintf(stderr, "[Cage|auto-scalar] FAIL: toy_add(10,3) = %d, expected 30\n", result);
        return 1;
    }
    printf("[Cage|auto-scalar] PASS: toy_add(10,3) = %d (intercepted as multiply)\n", result);
    return 0;
}
