
#include <libm-alias-double.h>
#include <math.h>

double
__cbrt (double x)
{
  return cbrt (x);
}

libm_alias_double (__cbrt, cbrt)
