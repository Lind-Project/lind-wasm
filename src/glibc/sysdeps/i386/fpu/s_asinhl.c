#include <libm-alias-ldouble.h>
#include <math.h>
#include <float.h> // For LDBL_MAX

long double
__asinhl (long double x)
{
  if (isnan (x))
    {
      return x; // Return NaN if input is NaN
    }
  if (isinf (x))
    {
      return x; // Return infinity if input is infinity
    }

  long double abs_x = fabsl (x);
  if (abs_x < 1e-34)
    {
      // For very small x, asinh(x) is approximately x
      return x;
    }
  else if (abs_x <= 2.0)
    {
      // Use the formula: asinh(x) = sign(x) * log1p(x + x*x / (1 + sqrt(1 +
      // x*x)))
      long double x_squared = x * x;
      return copysignl (
	  log1pl (x + x_squared / (1.0 + sqrtl (1.0 + x_squared))), x);
    }
  else
    {
      // Use the formula: asinh(x) = sign(x) * (log(2*x + 1/(x+sqrt(x*x+1))))
      return copysignl (logl (2.0 * x + 1.0 / (x + sqrtl (x * x + 1.0))), x);
    }
}

libm_alias_ldouble (__asinh, asinh)
