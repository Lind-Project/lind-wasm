#include <math.h> // Include the math library for scalbnf()

float
__scalbnf (float x, int n)
{
  return scalbnf (
      x, n); // Use the standard library function to scale by powers of 2
}
