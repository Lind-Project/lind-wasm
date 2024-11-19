
#include <libm-alias-float.h>
#include <math.h>  // For remquof function

float __remquof(float dividend, float divisor, int *quotient) {
    // The remquof function returns the remainder of the division of dividend by divisor,
    // and the quotient is stored in the location pointed to by quotient.
    return remquof(dividend, divisor, quotient);
}

libm_alias_float (__remquo, remquo)
