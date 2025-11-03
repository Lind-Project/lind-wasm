/*
 * Public domain.
 */

#include <math.h>
#include <fenv.h>

long double
myCeilL (long double x)
{
  // Save the current floating-point environment
  fenv_t env;
  fegetenv (&env);

  // Set rounding direction to upwards
  fesetround (FE_UPWARD);

  // Compute the ceiling of the input
  long double result = ceill (x);

  // Restore the previous floating-point environment
  fesetenv (&env);

  return result;
}
