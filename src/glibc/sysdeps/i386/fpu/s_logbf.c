
#include <libm-alias-float.h>
#include <math.h>
#include <float.h>

float
__logbf (float x)
{
  // Check for zero or subnormal numbers.
  if (x == 0.0f)
    {
      // Return -infinity for logb(0), and raise the divide-by-zero
      // floating-point exception.
      return -INFINITY;
    }
  else if (!isnormal (x))
    {
      // If x is subnormal, its exponent is less than the minimum of normal
      // numbers.
      return FLT_MIN_EXP - 1;
    }

  // Extract the unbiased exponent of x.
  int exponent;
  frexpf (x, &exponent);

  // frexp extracts as x = mantissa * 2^exponent, where mantissa is in [0.5,
  // 1). However, frexp returns 'exponent - 1' since the mantissa is normalized
  // to [0.5, 1), and we need to adjust it back.
  return (float) (exponent - 1);
}

libm_alias_float (__logb, logb)
