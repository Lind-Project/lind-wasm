#include <libm-alias-finite.h>
#include <math.h>
#include <float.h>
#include <limits.h>

long double __ieee754_log2l(long double x) {
    // Handling NaN and infinity according to IEEE 754
    if (isnan(x)) {
        return NAN;  // Return NaN if input is NaN
    }
    if (x < 0.0L) {
        return NAN;  // log2 is not defined for negative values
    }
    if (x == 0.0L) {
        return -HUGE_VALL;  // log2(0) is -infinity
    }
    if (isinf(x)) {
        return HUGE_VALL;  // log2(infinity) is infinity
    }

    // Special handling for values close to 1.0 to improve precision
    long double delta = x - 1.0L;
    if (fabs(delta) <= 0.29L) {
        // A simplified version of the Taylor series expansion around 1
        // This may need adjustment to match specific precision requirements
        return delta - (delta * delta) / 2.0L + (delta * delta * delta) / 3.0L;
    }

    // General computation using the standard library function
    return log2l(x);  // Use the standard library function for other cases
}
libm_alias_finite (__ieee754_log2l, __log2l)
