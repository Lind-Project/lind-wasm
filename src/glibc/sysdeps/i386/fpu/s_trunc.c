
// #include <libm-alias-double.h>
#include <math.h> // Include the math library for trunc()

double
Mytrunc (double x)
{
  return trunc (x); // Use the standard library function
}

// libm_alias_double (__trunc, trunc)
