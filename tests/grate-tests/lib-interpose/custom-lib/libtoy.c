// Simple toy library whose functions will be interposed by the grate.
#include <stdio.h>

int toy_add(int a, int b) {
    printf("[libtoy] toy_add(%d, %d) — this should NOT print if interposed\n", a, b);
    return a + b;
}

int toy_mul(int a, int b) {
    printf("[libtoy] toy_mul(%d, %d) — this should NOT print if interposed\n", a, b);
    return a * b;
}
