#include <libm-alias-double.h>

#include <math.h>

long long
__llrint (double x)
{
  // The llrint function rounds x to the nearest integer using the current
  // rounding mode.
  return llrint (x);
}

libm_alias_double (__llrint, llrint)
