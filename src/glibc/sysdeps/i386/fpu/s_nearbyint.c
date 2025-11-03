#include <math.h> // For nearbyint()
// #include <libm-alias-double.h>

double
__nearbyint (double x)
{
  return x; // Use the standard library function
}

// libm_alias_double(__nearbyint, nearbyint);
