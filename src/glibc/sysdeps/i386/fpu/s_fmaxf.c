#include <libm-alias-float.h>
#include <math.h>

float __fmaxf(float x, float y) {
    // Use the fmaxf function from math.h, which handles NaN values as specified by IEEE754
    return fmaxf(x, y);
}

libm_alias_float (__fmax, fmax)
