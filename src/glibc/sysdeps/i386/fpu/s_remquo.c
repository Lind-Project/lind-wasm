
#include <libm-alias-double.h>
#include <math.h>  // For remquo function

double __remquo(double dividend, double divisor, int *quotient) {
    // The remquo function returns the remainder of the division of dividend by divisor
    // and the quotient is stored in the location pointed to by quotient.
    return remquo(dividend, divisor, quotient);
}

libm_alias_double (__remquo, remquo)
