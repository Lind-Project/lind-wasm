#include <libm-alias-finite.h>
#include <math.h>
#include <float.h>
#include <errno.h>
#include <fenv.h>

double __ieee754_powl(long double x, long double y) {
    // Check for NaNs
    if (isnan(x) || isnan(y)) {
        return x + y; // propagate NaNs
    }

    // Check for exact zeros in y
    if (y == 0.0L) {
        return 1.0; // pow(x, 0) == 1
    }

    // Check for x == 0
    if (x == 0.0L) {
        if (y > 0.0L) {
            return 0.0L;
        } else {
            return HUGE_VALL; // Return positive infinity for negative exponents
        }
    }

    // Check for x == 1 or y == +inf or y == -inf
    if (x == 1.0L || isinf(y)) {
        if (isinf(x)) {
            return x; // 1 raised to any power is 1
        }
        if (y > 0) {
            return HUGE_VALL;
        } else {
            return 0.0L;
        }
    }

    // Check for x == +inf or x == -inf
    if (isinf(x)) {
        if (x > 0 || fmodl(y, 2.0L) == 0.0) {
            return HUGE_VALL;
        } else {
            return -HUGE_VALL;
        }
    }

    // For negative base x
    if (x < 0.0L) {
        if (y != floorl(y)) { // y must be an integer
            errno = EDOM;
            return NAN;
        }
        long double result = expl(y * logl(fabsl(x)));
        if (fmodl(y, 2.0L) != 0.0L) { // Adjust sign if y is odd
            result = -result;
        }
        return result;
    }

    // General case
    return expl(y * logl(x));
}

libm_alias_finite (__ieee754_powl, __powl)
