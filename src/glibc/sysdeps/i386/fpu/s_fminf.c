
#include <libm-alias-float.h>
#include <math.h>

float
__fminf (float x, float y)
{
  // Use the fminf function from math.h which handles NaN values as specified
  // by IEEE754
  return fminf (x, y);
}

libm_alias_float (__fmin, fmin)
