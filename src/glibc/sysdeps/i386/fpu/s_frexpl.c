

#include <libm-alias-ldouble.h>

#include <math.h>

long double
__frexpl (long double value, int *exp)
{
  // The frexpl function decomposes a long double floating point number into a
  // normalized fraction and an exponent. The normalized fraction (mantissa) is
  // returned, and the exponent is stored in the integer pointed to by exp.
  return frexpl (value, exp);
}

libm_alias_ldouble (__frexp, frexp)
