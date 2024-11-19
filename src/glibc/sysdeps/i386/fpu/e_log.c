#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
#include <float.h>
#include <limits.h>

double __ieee754_log(double x) {
    if (isnan(x)) {
        return NAN;  // Return NaN if input is NaN
    }
    if (x < 0.0) {
        return NAN;  // Logarithm is not defined for negative values
    }
    if (x == 0.0) {
        return -HUGE_VAL;  // Log(0) is -infinity
    }
    if (isinf(x)) {
        return HUGE_VAL;  // Log(infinity) is infinity
    }

    // Check for values very close to 1 using a precise epsilon
    double delta = x - 1.0;
    if (fabs(delta) <= 0.29) {  // Threshold from original assembly, handling precision near 1
        return delta - 0.5 * delta * delta + (delta * delta * delta) / 3.0; // Use a Taylor expansion for small delta
    }

    // General computation
    return log(x); // Use the standard library function
}
libm_alias_finite (__ieee754_log, __log)
