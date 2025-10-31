#include <libm-alias-finite.h>
#include <math.h>
#include <errno.h>

long double __ieee754_scalb(long double x, long double y) {
    // Check for NaN or infinity on y
    if (isnan(y) || isinf(y)) {
        if (isnan(x)) {
            return x; // Return NaN if x is also NaN
        }
        if (isinf(y)) {
            if (y > 0) {
                return x > 0 ? HUGE_VALL : -HUGE_VALL;
            } else {
                return x > 0 ? 0.0L : -0.0L;
            }
        }
        return nan(""); // y is NaN, return NaN
    }

    // Check for NaN on x
    if (isnan(x)) {
        return x; // Return x if it is NaN
    }

    // Calculate the scaling factor as an integer
    long exp = (long) y;
    if (y - (long double)exp != 0.0L) {
        errno = EDOM; // y is not integral, set domain error
        return nan(""); // Return NaN because of non-integral scale factor
    }

    // Scale x by 2^y using scalbln, which is designed for long integer exponents
    return scalbln(x, exp);
}

libm_alias_finite (__ieee754_scalb, __scalb)

// lind-wasm: added wrapper function for wasm compilation
double scalb(double x, double y) {
  return __ieee754_scalb(x, y);
}
