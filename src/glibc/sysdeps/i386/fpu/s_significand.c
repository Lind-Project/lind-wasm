#include <math.h>  // Include the math library for frexp()

double __significand(double x) {
    int exponent;  // Variable to hold the exponent which we will ignore
    double significand = frexp(x, &exponent);  // Extract the significand and exponent
    // The significand returned by frexp() is in the range [0.5, 1.0), or zero.
    // However, significand() needs the significand in the range [1.0, 2.0), so we adjust it:
    if (significand != 0.0) {
        significand *= 2.0;
        exponent--;  // Adjust the exponent since we multiplied the significand by 2
    }
    return significand;
}
