#include <math.h>
#include <float.h>
#include <limits.h>
#include <libm-alias-finite.h>

long double __ieee754_log10l(long double x) {
    // Handling NaN and infinity according to IEEE 754
    if (isnan(x)) {
        return NAN;  // Return NaN if input is NaN
    }
    if (x < 0.0L) {
        return NAN;  // log10 is not defined for negative values
    }
    if (x == 0.0L) {
        return -HUGE_VALL;  // log10(0) is -infinity
    }
    if (isinf(x)) {
        return HUGE_VALL;  // log10(infinity) is infinity
    }

    // Special handling for values close to 1.0 to improve precision
    long double delta = x - 1.0L;
    if (fabs(delta) <= 0.29L) {
        // Use a Taylor series expansion for small delta
        // Approximation: log10(1 + delta) ≈ delta / ln(10) - (delta^2) / (2 * ln(10)) + ...
        // We compute this using the identity: log10(x) = log(x) / log(10)
        long double log_delta = delta - (delta * delta) / 2.0L + (delta * delta * delta) / 3.0L;
        return log_delta / log(10.0L);
    }

    // General case
    return log10l(x);  // Use the standard library function
}

libm_alias_finite (__ieee754_log10l, __log10l)

double log10l(double x) {
  return __ieee754_log10l(x);
}
