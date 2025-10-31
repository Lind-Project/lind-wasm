#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
#include <float.h>

long double __ieee754_exp2l(long double x) {
    // Check for NaN or infinity
    if (isnan(x)) {
        return x;  // NaN should return NaN
    }
    if (isinf(x)) {
        if (x > 0) {
            return INFINITY;  // exp2(inf) = inf
        } else {
            return 0.0L;  // exp2(-inf) = 0
        }
    }

    // Check for very small values of x where result rounds to 1
    if (x < -65.0L) {
        return 1.0L;  // For x < -65, 2^x rounds to 1 in long double precision
    }

    // Normal computation for other values
    long double int_part;
    long double fract_part = modfl(x, &int_part);  // Separate integer and fractional parts
    long double exp2_fract = exp2l(fract_part);    // 2 raised to the fractional part
    return ldexpl(exp2_fract, (int)int_part);      // Scale the result by 2 raised to the integer part
}

libm_alias_finite (__ieee754_exp2l, __exp2l)

double exp2l(double x) {
  return __ieee754_exp2l(x);
}
