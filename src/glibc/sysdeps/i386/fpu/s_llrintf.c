
#include <libm-alias-float.h>

#include <math.h>

long long
__llrintf (float x)
{
  // The llrintf function rounds x to the nearest integer using the current
  // rounding mode.
  return llrintf (x);
}

libm_alias_float (__llrint, llrint)
