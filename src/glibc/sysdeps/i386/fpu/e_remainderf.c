
#include <math.h>

float __ieee754_remainderf(float x, float y) {
    return remainderf(x, y);
}



double remainderf(double x) {
  return __ieee754_remainderf(x);
}
