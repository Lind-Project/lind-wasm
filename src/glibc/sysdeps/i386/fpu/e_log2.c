#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
#include <float.h>
#include <limits.h>

double __ieee754_log2(double x) {
    // Check for NaN and infinity according to IEEE 754
    if (isnan(x)) {
        return NAN;  // NaN should return NaN
    }
    if (x < 0.0) {
        return NAN;  // log2 is not defined for negative values
    }
    if (x == 0.0) {
        return -HUGE_VAL;  // log2(0) is -infinity
    }
    if (isinf(x)) {
        return HUGE_VAL;  // log2(infinity) is infinity
    }

    // For values very close to 1, use a more precise approach
    if (fabs(x - 1.0) <= 0.29) {
        return (x - 1.0) / (x + 1.0) * 2.0 + (x - 1.0) * (x - 1.0) / (x + 1.0) * (x + 1.0) / 3.0;
    }

    // General case
    return log2(x);  // Use the standard library function
}
libm_alias_finite (__ieee754_log2, __log2)
