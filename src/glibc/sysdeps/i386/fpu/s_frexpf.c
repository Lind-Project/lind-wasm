
#include <libm-alias-float.h>
#include <math.h>

float
__frexpf (float value, int *exp)
{
  // The frexpf function decomposes a floating point number into a normalized
  // fraction and an exponent. The normalized fraction (mantissa) is returned,
  // and the exponent is stored in the integer pointed to by exp.
  return frexpf (value, exp);
}

libm_alias_float (__frexp, frexp)
