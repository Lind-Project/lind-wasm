#include <math.h> // For lrint()
#include <libm-alias-double.h>

// Alias the function name as per the original requirement, if needed
long
__lrint (double x)
{
  return lrint (x); // Use the standard library function
}

libm_alias_double (__lrint, lrint);
