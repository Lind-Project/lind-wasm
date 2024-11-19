#include <libm-alias-double.h>
#include <math.h>

double __fmin(double x, double y) {
    // Use the fmin function from math.h which handles NaN values as specified by IEEE754
    return fmin(x, y);
}

libm_alias_double (__fmin, fmin)
