#include <libm-alias-finite.h>
#include <math.h>

float __ieee754_sqrtf(float x) {
    // Check for negative input to handle domain error
    if (x < 0) {
        // IEEE standard dictates to return NaN for negative arguments
        return NAN;
    }

    // Calculate the square root of the input
    return sqrtf(x);
}

libm_alias_finite (__ieee754_sqrtf, __sqrtf)


double sqrtf(double x) {
  return __ieee754_sqrtf(x);
}
