#include <libm-alias-double.h>
#include <math.h>

double
__fmax (double x, double y)
{
  // Use the fmax function which considers NaNs and returns the maximum of the
  // two values
  return fmax (x, y);
}

libm_alias_double (__fmax, fmax)
