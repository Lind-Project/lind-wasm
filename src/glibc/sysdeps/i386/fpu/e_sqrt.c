#include <libm-alias-finite.h>
#include <math.h>
#include <fenv.h>

double
__ieee754_sqrt (double x)
{
  // First, check for negative input, which is domain error in standard sqrt
  if (x < 0)
    {
      feraiseexcept (FE_INVALID); // Raise the invalid floating-point exception
      return nan ("");		  // Return NaN as per IEEE standard
    }

  // Save the current floating-point environment
  fenv_t env;
  fegetenv (&env);

  // Set the rounding mode to the nearest (similar to setting control word in
  // assembly)
  fesetround (FE_TONEAREST);

  // Compute the square root
  double result = sqrt (x);

  // Restore the floating-point environment
  fesetenv (&env);

  return result;
}

libm_alias_finite (__ieee754_sqrt, __sqrt)
