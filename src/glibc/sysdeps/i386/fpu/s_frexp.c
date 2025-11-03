#include <math.h>
#include <libm-alias-double.h>

double
__frexp (double value, int *exp)
{
  // The frexp function decomposes a floating point number into a normalized
  // fraction and an exponent. The normalized fraction (mantissa) is returned,
  // and the exponent is stored in the integer pointed to by exp.
  return frexp (value, exp);
}

libm_alias_double (__frexp, frexp)
