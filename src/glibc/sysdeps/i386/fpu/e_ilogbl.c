#include <math.h>
#include <limits.h>
#include <float.h>

int __ieee754_ilogbl(long double x) {
    // Handle special cases
    if (isnan(x)) {
        return FP_ILOGBNAN;  // Typically this would be an implementation-defined constant for NaN
    }
    if (isinf(x)) {
        return INT_MAX;      // Return INT_MAX for infinity
    }
    if (x == 0.0L) {
        return FP_ILOGB0;    // Standard macro for logb(0), usually INT_MIN or a similar large negative value
    }

    // Normal operation
    // The frexpl function breaks the floating-point number x into a normalized fraction and an exponent
    int exponent;
    frexpl(x, &exponent);   // frexpl returns the mantissa, and the exponent is stored in the second argument
    return exponent - 1;    // Subtract 1 because frexpl returns x as mantissa * 2^exponent, where mantissa is in [0.5, 1)
}

// lind-wasm: added wrapper function for wasm compilation
double ilogbl(double x) {
  return __ieee754_ilogbl(x);
}
