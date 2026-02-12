#include <math.h>
#include <stdio.h>

int main(void) {
    double (*cos_fn)(double) = cos;
    double (*sqrt_fn)(double) = sqrt;

    double value = cos_fn(0.0) + sqrt_fn(16.0);

    if (value > 4.9999 && value < 5.0001) {
        puts("math_link_smoke: ok");
        return 0;
    }

    puts("math_link_smoke: bad value");
    return 1;
}
