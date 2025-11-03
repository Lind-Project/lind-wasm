#include <math.h>
#include <float.h>

float
__log1pf (float x)
{
  // Define the range limit for the specialized approximation.
  const float limit = 0.29f;
  // const float one = 1.0f;
  float result;

  if (isnan (x) || isinf (x))
    {
      // Handle special cases:
      // log1p(-1) should yield -Infinity (logarithm of zero).
      // log1p(-Inf) should yield NaN (logarithm of a negative number is not
      // defined). log1p(Inf) should yield Inf.
      result = (x == -INFINITY) ? NAN : x;
    }
  else if (fabsf (x) <= limit)
    {
      // For x within the range -0.29 to 0.29, use the precise method.
      // This range allows the usage of an approximation that reduces
      // computational errors.
      result = log1pf (x);
    }
  else
    {
      // For values outside the specialized range, calculate directly.
      // This involves more straightforward computation using the standard log
      // function, which is accurate enough outside the critical range around
      // zero.
      result = logf (1.0f + x);
    }

  // Ensure the underflow behavior is handled correctly.
  return result;
}
