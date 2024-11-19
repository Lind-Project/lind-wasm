#include <math.h>  // Include the math library for frexpf()

float __significandf(float x) {
    int exponent;  // Variable to hold the exponent which we will ignore
    float significand = frexpf(x, &exponent);  // Extract the significand and exponent
    // The significand returned by frexpf() is in the range [0.5, 1.0), or zero.
    // However, significandf() needs the significand in the range [1.0, 2.0), so we adjust it:
    if (significand != 0.0) {
        significand *= 2.0f;
        exponent--;  // Adjust the exponent since we multiplied the significand by 2
    }
    return significand;
}

