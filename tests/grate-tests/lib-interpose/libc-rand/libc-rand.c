// Cage app: calls rand() from libc.
// When running under the libc-rand grate, rand() is intercepted and always
// returns 42 instead of a pseudo-random value.
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>

int main(void) {
    // Call rand() three times. Under interposition all three must return 42.
    // The real rand() would return different values, so 42 proves interposition.
    for (int i = 0; i < 3; i++) {
        int r = rand();
        printf("[Cage] rand() = %d\n", r);
        if (r != 42) {
            fprintf(stderr, "[Cage] FAIL: expected 42 (interposed), got %d\n", r);
            assert(0);
        }
    }

    printf("[Cage] PASS\n");
    return 0;
}
