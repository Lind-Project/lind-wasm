#include <libm-alias-finite.h>
#include <math.h>
#include <errno.h>
#include <float.h>

float __ieee754_scalbf(float x, float y) {
    // Check for NaN or infinity on y
    if (isnan(y) || isinf(y)) {
        if (isnan(x)) {
            return x; // Return NaN if x is also NaN
        }
        if (isinf(y)) {
            if (y > 0) {
                return x > 0 ? HUGE_VALF : -HUGE_VALF;
            } else {
                return x > 0 ? 0.0f : -0.0f;
            }
        }
        return nanf(""); // y is NaN, return NaN
    }

    // Check for NaN on x
    if (isnan(x)) {
        return x; // Return x if it is NaN
    }

    // Convert y to an integer for scaling
    long exp = (long) y;
    if (y - (float)exp != 0.0f) {
        errno = EDOM; // y is not integral, set domain error
        return nanf(""); // Return NaN due to non-integral scale factor
    }

    // Use the scalbnf function, which is designed for floating point numbers and integer exponents
    return scalbnf(x, exp);
}

libm_alias_finite (__ieee754_scalbf, __scalbf)

double scalbf(double x) {
  return __ieee754_scalbf(x);
}
