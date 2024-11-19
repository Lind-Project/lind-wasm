#include <math.h>  // Include for lrintl()
//#include <libm-alias-ldouble.h>

long double Mylrintl(long double x) {
    // Convert long double to nearest integer using the current rounding mode.
    return lrintl(x);
}

//libm_alias_ldouble(__lrintl, lrint);

