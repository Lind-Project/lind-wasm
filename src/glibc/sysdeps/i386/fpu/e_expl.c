#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
#include <float.h>

// Assuming definitions for USE_AS_EXP10L and similar are provided elsewhere
// Define constants with more precision if necessary
#ifndef M_LOG2E
#define M_LOG2E 1.44269504088896340736 // log2(e)
#endif

#ifndef M_LOG2_10
#define M_LOG2_10 3.32192809488736234787 // log2(10)
#endif

// This function prototype changes depending on what's being compiled
long double ieee754_expl_general(long double x, long double base_log2) {
    // Check for special values
    if (isnan(x)) {
        return x; // Return NaN if input is NaN
    }
    if (isinf(x)) {
        if (x > 0) {
            return INFINITY; // Positive infinity for exp of positive infinity
        } else {
            return 0.0L; // Zero for exp of negative infinity
        }
    }

    // Perform the computation of base^x using 2^(x * log2(base))
    long double x_log2_base = x * base_log2; // Convert to log base 2
    long double int_part;
    long double fract_part = modfl(x_log2_base, &int_part); // Separate integer and fractional parts
    long double exp2_fract = exp2l(fract_part); // 2^fract_part
    return ldexpl(exp2_fract, (int)int_part); // Scale the result by 2^int_part
}

long double ieee754_expl(long double x) {
    return ieee754_expl_general(x, M_LOG2E);
}

long double __ieee754_expl(long double x) {
    return ieee754_expl(x);
}
libm_alias_finite (__ieee754_expl, __expl)


double expl(double x) {
  return __ieee754_expl(x);
}
