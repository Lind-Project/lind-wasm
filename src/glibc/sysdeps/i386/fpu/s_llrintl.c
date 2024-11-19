#include <libm-alias-ldouble.h>
#include <math.h>

long long __llrintl(long double x) {
    // The llrintl function rounds x to the nearest integer using the current rounding mode.
    return llrintl(x);
}

libm_alias_ldouble (__llrint, llrint)
