/*
 * Public domain.
 */

#include <libm-alias-ldouble.h>
#include <math.h>  // For nearbyint() function
#include <float.h> // For handling floating-point limits and properties

long double
__nearbyintl (long double x)
{
  // The nearbyint function will round x to the nearest integer using the
  // current rounding mode
  return nearbyintl (x);
}

libm_alias_ldouble (__nearbyint, nearbyint)
