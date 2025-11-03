#include <math.h>

int
__finite (double x)
{
  // Use the isfinite macro from math.h to check if the number is finite
  return isfinite (x);
}
