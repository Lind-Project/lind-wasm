
#include <libm-alias-finite.h>
#include <math.h>
#include <float.h>
#include <limits.h>

long double __ieee754_logl(long double x) {
    // Handling NaN and infinity according to IEEE 754
    if (isnan(x)) {
        return NAN;  // Return NaN if input is NaN
    }
    if (x < 0.0L) {
        return NAN;  // log is not defined for negative values
    }
    if (x == 0.0L) {
        return -HUGE_VALL;  // log(0) is -infinity
    }
    if (isinf(x)) {
        return HUGE_VALL;  // log(infinity) is infinity
    }

    // Special handling for values close to 1.0 to improve precision
    long double delta = x - 1.0L;
    if (fabs(delta) <= 0.29L) {
        // A simplified Taylor series expansion for log(x) about 1:
        // log(x) ≈ (x - 1) - 0.5 * (x - 1)^2 + (x - 1)^3 / 3 - ...
        // For a more accurate computation near 1, we use the first three terms.
        return delta - 0.5L * delta * delta + (delta * delta * delta) / 3.0L;
    }

    // General case
    return logl(x);  // Use the standard library function
}

libm_alias_finite (__ieee754_logl, __logl)

double logl(double x) {
  return __ieee754_logl(x);
}
