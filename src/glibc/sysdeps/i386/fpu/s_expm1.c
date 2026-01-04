#include <math.h>
#include <float.h>
#include <fenv.h>

double __expm1(double x) {
    // Handle special cases directly
    if (isnan(x) || x == INFINITY) {
        return x; // expm1(INFINITY) is INFINITY
    } else if (x == -INFINITY) {
        return -1.0; // expm1(-INFINITY) is -1
    } else if (x == 0.0) {
        return 0.0; // expm1(0) is exactly 0
    }

    // Check if x is in a range that can lead to underflow or overflow
    if (x >= 710.0) { // Log base e of DBL_MAX approximately
        return INFINITY; // Overflows
    } else if (x <= -37.42994775023705) { // Log base e of DBL_MIN approximately
        return -1.0; // Underflows to -1 accurately
    }

    // Use the exp function from the math library for accurate calculation
    double e_to_x = exp(x);

    // Correct for subtraction of 1 near zero to improve precision
    if (fabs(x) < 1e-5) {
        return x + 0.5 * x * x; // Use series expansion near zero
    } else {
        return e_to_x - 1.0; // Standard case
    }
}

weak_alias (__expm1, expm1)
