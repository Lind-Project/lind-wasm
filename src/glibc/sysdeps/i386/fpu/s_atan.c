#include <libm-alias-double.h>

#include <math.h>

double __atan(double x) {
    return atan(x);
}

libm_alias_double (__atan, atan)
