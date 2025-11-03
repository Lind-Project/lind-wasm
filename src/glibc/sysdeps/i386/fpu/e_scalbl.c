#include <libm-alias-finite.h>
#include <math.h>
#include <errno.h>
#include <float.h>

long double
__ieee754_scalbl (long double x, long double y)
{
  // Check for NaN or infinity on y
  if (isnan (y) || isinf (y))
    {
      if (isnan (x))
	{
	  return x; // Return NaN if x is also NaN
	}
      if (isinf (y))
	{
	  if (y > 0)
	    {
	      return x > 0 ? HUGE_VALL : -HUGE_VALL;
	    }
	  else
	    {
	      return x > 0 ? 0.0L : -0.0L;
	    }
	}
      return nanl (""); // y is NaN, return NaN
    }

  // Check for NaN on x
  if (isnan (x))
    {
      return x; // Return x if it is NaN
    }

  // Convert y to an integer for scaling
  long exp = (long) y;
  if (y - (long double) exp != 0.0L)
    {
      errno = EDOM;	// y is not integral, set domain error
      return nanl (""); // Return NaN due to non-integral scale factor
    }

  // Use the scalbln function, which is designed for floating point numbers and
  // integer exponents
  return scalbln (x, exp);
}

libm_alias_finite (__ieee754_scalbl, __scalbl)
