#include <stdio.h>
#include <math.h>

int main(void) {
    double x = 12.8;
    int exponent;

    // frexp splits x into mantissa and exponent
    // x = mantissa * 2^exponent
    double mantissa = frexp(x, &exponent);

    printf("Original number: %f\n", x);
    printf("Mantissa: %f\n", mantissa);
    printf("Exponent: %d\n", exponent);

    return 0;
}
