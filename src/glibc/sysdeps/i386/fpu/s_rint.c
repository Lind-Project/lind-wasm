
// #include <libm-alias-double.h>
#include <math.h> // Include the math library for rint()

double
Myrint (double x)
{
  return rint (
      x); // Use the standard library function to round to the nearest integer
}

// libm_alias_double (__rint, rint)
