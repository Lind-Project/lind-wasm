#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Test that printf float format specifiers produce correct output
 * instead of NAN. Regression test for ldbl-96 → ldbl-128 fix
 * (wasm32 long double is 128-bit IEEE quad, not x86 80-bit).
 */

int main(void)
{
    char buf[256];

    /* %f — basic double */
    snprintf(buf, sizeof(buf), "%f", 1.0);
    assert(strcmp(buf, "1.000000") == 0);

    /* %F — uppercase */
    snprintf(buf, sizeof(buf), "%F", 1.0);
    assert(strcmp(buf, "1.000000") == 0);

    /* %e — scientific */
    snprintf(buf, sizeof(buf), "%e", 2.0);
    assert(strcmp(buf, "2.000000e+00") == 0);

    /* %E — uppercase scientific */
    snprintf(buf, sizeof(buf), "%E", 2.0);
    assert(strcmp(buf, "2.000000E+00") == 0);

    /* %g — shortest representation */
    snprintf(buf, sizeof(buf), "%g", 1.5);
    assert(strcmp(buf, "1.5") == 0);

    /* %g — large value switches to scientific */
    snprintf(buf, sizeof(buf), "%g", 1e10);
    assert(strcmp(buf, "1e+10") == 0);

    /* precision */
    snprintf(buf, sizeof(buf), "%.2f", 3.14159);
    assert(strcmp(buf, "3.14") == 0);

    /* negative */
    snprintf(buf, sizeof(buf), "%f", -42.5);
    assert(strcmp(buf, "-42.500000") == 0);

    /* zero */
    snprintf(buf, sizeof(buf), "%f", 0.0);
    assert(strcmp(buf, "0.000000") == 0);

    /* long double via %Lf */
    long double ld = 1.0L;
    snprintf(buf, sizeof(buf), "%Lf", ld);
    assert(strcmp(buf, "1.000000") == 0);

    snprintf(buf, sizeof(buf), "%Le", (long double)2.0);
    assert(strcmp(buf, "2.000000e+00") == 0);

    printf("printf_float: all tests passed\n");
    return 0;
}
