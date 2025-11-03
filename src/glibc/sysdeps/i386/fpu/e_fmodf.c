#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>

float
__ieee754_fmodf (float x, float y)
{
  return fmodf (x, y);
}
libm_alias_finite (__ieee754_fmodf, __fmodf)
