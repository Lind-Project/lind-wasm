// Cage app: calls toy_add and toy_mul from the dynamically linked libtoy.
// When running under the lib-interpose grate, both calls are intercepted.
#include <stdio.h>
#include <assert.h>

// Declarations for the dynamically linked toy library functions.
int toy_add(int a, int b);
int toy_mul(int a, int b);

int main(void) {
    int r1 = toy_add(3, 4);
    printf("[Cage] toy_add(3, 4) = %d\n", r1);
    // Grate handler returns (a + b) * 2 for toy_add to prove interposition.
    if (r1 != 14) {
        fprintf(stderr, "[Cage] FAIL: expected 14 (interposed), got %d\n", r1);
        assert(0);
    }

    int r2 = toy_mul(5, 6);
    printf("[Cage] toy_mul(5, 6) = %d\n", r2);
    // Grate handler returns a + b (instead of a * b) for toy_mul.
    if (r2 != 11) {
        fprintf(stderr, "[Cage] FAIL: expected 11 (interposed), got %d\n", r2);
        assert(0);
    }

    printf("[Cage] PASS\n");
    return 0;
}
