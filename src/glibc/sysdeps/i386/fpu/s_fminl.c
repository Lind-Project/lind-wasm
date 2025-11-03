
#include <libm-alias-ldouble.h>
#include <math.h>

long double
__fminl (long double x, long double y)
{
  // Use the fminl function from math.h which handles NaN values correctly
  return fminl (x, y);
}

libm_alias_ldouble (__fmin, fmin)
