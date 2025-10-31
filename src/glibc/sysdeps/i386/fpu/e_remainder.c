#include <libm-alias-finite.h>
#include <math.h>

double __ieee754_remainder(double x, double y) {
    return remainder(x, y);
}

libm_alias_finite (__ieee754_remainder, __remainder)

// lind-wasm: added wrapper function for wasm compilation
double remainder(double x, double y) {
  return __ieee754_remainder(x, y);
}
