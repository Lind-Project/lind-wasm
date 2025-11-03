#include <libm-alias-finite.h>
#include <math.h>
#include <float.h>
#include <errno.h>

double
__ieee754_pow (double x, double y)
{
  if (isnan (y))
    {
      return y; // If y is NaN, the result is NaN
    }
  if (y == 0.0)
    {
      return 1.0; // pow(x, 0) is always 1
    }
  if (isnan (x))
    {
      return x; // If x is NaN, the result is NaN
    }

  if (x == 0.0)
    {
      if (y < 0.0)
	{
	  if (fmod (y, 2.0) == 0.0)
	    {
	      return HUGE_VAL; // pow(0, negative even integer) is +infinity
	    }
	  else
	    {
	      return -HUGE_VAL; // pow(0, negative odd integer) is -infinity
	    }
	}
      return 0.0; // pow(0, positive) is 0
    }

  if (isinf (x))
    {
      if (y < 0.0)
	{
	  return 0.0; // pow(inf, negative) is 0
	}
      if (fmod (y, 2.0) == 0.0)
	{
	  return HUGE_VAL; // pow(inf, positive even integer) is +infinity
	}
      else
	{
	  return copysign (
	      HUGE_VAL,
	      x); // pow(inf, positive odd integer) follows the sign of x
	}
    }

  if (x < 0.0)
    {
      // Handling negative bases with non-integer exponent
      if (y != floor (y))
	{
	  errno = EDOM; // Domain error for negative base with non-integer
			// exponent
	  return NAN;
	}
      // Calculate power for negative base with integer exponent
      double result = exp (y * log (fabs (x)));
      if (fmod (y, 2.0) == 1.0)
	{ // Adjust sign if exponent is odd
	  return -result;
	}
      return result;
    }

  // General case for positive x and any y
  return exp (y * log (x));
}

libm_alias_finite (__ieee754_pow, __pow)
