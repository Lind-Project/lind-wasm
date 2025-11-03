#include <libm-alias-ldouble.h>
#include <math.h>

long double
__fmaxl (long double x, long double y)
{
  // Use the fmaxl function from math.h, which handles NaN values as specified
  // by IEEE754
  return fmaxl (x, y);
}

libm_alias_ldouble (__fmax, fmax)
