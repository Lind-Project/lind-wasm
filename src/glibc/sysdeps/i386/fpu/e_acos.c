#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
double __ieee754_acos(double x) {
    if (x < -1.0 || x > 1.0) {
        // Handle domain error, e.g., set errno to EDOM, return NaN or similar as per your environment
        return NAN;
    }

    return atan2(sqrt(1 - x * x), x);
}
libm_alias_finite (__ieee754_acos, __acos)

// lind-wasm: added wrapper function for wasm compilation
double acos(double x) {
  return __ieee754_acos(x);
}
