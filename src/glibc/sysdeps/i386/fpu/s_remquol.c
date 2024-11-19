/*
 * Public domain.
 */

#include <libm-alias-ldouble.h>
#include <math.h>  // For remquol function

long double __remquol(long double dividend, long double divisor, int *quotient) {
    // The remquol function returns the remainder of the division of dividend by divisor,
    // and the quotient is stored in the location pointed to by quotient.
    return remquol(dividend, divisor, quotient);
}

libm_alias_ldouble (__remquo, remquo)
