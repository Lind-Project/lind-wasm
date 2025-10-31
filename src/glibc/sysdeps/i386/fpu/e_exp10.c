#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
#include <float.h>

double __ieee754_exp10(double x) {
    // Handle special input values of NaN and infinity
    if (isnan(x)) {
        return x;  // NaN should return NaN
    }
    if (isinf(x)) {
        if (x > 0) {
            return INFINITY;  // 10^inf = inf
        } else {
            return 0.0;  // 10^-inf = 0
        }
    }

    // Calculation for normal values
    double x_log2_10 = x * M_LOG2E * log(10.0); // Convert log base e to log base 2 using log2(10) = log(10) / log(2)
    double int_part;
    double fract_part = modf(x_log2_10, &int_part); // Separate integer and fractional parts
    double exp2_fract = exp2(fract_part);           // Compute 2^fract_part
    return ldexp(exp2_fract, (int)int_part);        // Scale the result by 2^int_part
}

libm_alias_finite (__ieee754_exp10, __exp10)

double exp10(double x) {
  return __ieee754_exp10(x);
}
