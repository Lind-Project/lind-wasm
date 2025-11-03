#include <math.h>
#include <float.h>
#include <errno.h>

float
__expm1f (float x)
{
  // Handle special cases explicitly for IEEE float compliance
  if (isnan (x))
    {
      return x; // expm1(NaN) is NaN
    }
  else if (x == INFINITY)
    {
      return INFINITY; // expm1(+inf) is +inf
    }
  else if (x == -INFINITY)
    {
      return -1.0f; // expm1(-inf) is -1
    }

  // Check if x is out of the bounds where the precise calculation is possible
  if (x >= 88.5f)
    {
      // Overflow range: exp(x) would overflow
      errno = ERANGE;
      return HUGE_VALF;
    }
  else if (x <= -18.0f)
    {
      // Underflow range: exp(x) would underflow
      errno = ERANGE;
      return -1.0f;
    }

  // Calculate expm1 using a more accurate method for small x
  if (fabsf (x) < 1e-5)
    {
      // Use Taylor expansion for small x: expm1(x) = x + x^2/2 + O(x^3)
      return x + 0.5f * x * x;
    }
  else
    {
      // For larger x, use the standard expm1 which should be precise enough
      return expf (x) - 1.0f;
    }
}
