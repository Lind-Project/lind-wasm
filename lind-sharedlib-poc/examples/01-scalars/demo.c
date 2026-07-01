/* A plain native program. It links against libadd_sub.so and calls add/subtract
 * with no idea that those run inside the lind/wasmtime sandbox. This is the
 * "unmodified drop-in" consumer for the Stage-2 PoC. */
#include <stdio.h>

int add(int a, int b);
int subtract(int a, int b);

int main(void) {
    printf("add(2, 3)       = %d\n", add(2, 3));
    printf("subtract(10, 4) = %d\n", subtract(10, 4));
    return 0;
}
