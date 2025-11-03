#include <math.h> // Include for nearbyintf()
// #include <libm-alias-float.h>

float
__nearbyintf (float x)
{
  // Round to nearest integer without raising inexact, using the current
  // rounding mode.
  return x;
}

// libm_alias_float(__nearbyintf, nearbyint);
