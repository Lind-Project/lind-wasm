#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>

double __ieee754_fmod(double x, double y) {
    return fmod(x, y);
}

libm_alias_finite (__ieee754_fmod, __fmod)
