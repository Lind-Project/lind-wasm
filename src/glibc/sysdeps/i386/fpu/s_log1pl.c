#include <math.h>
#include <float.h>

long double __log1pl(long double x) {
    // Define the range limit for the specialized approximation.
    const long double limit = 0.29L;
    //const long double one = 1.0L;
    long double result;

    if (isnan(x) || isinf(x)) {
        // Handle special cases:
        // log1p(-1) should yield -Infinity (logarithm of zero).
        // log1p(-Inf) should yield NaN (logarithm of a negative number is not defined).
        // log1p(Inf) should yield Inf.
        result = (x == -INFINITY) ? NAN : x;
    } else if (fabsl(x) <= limit) {
        // For x within the range -0.29 to 0.29, use the precise method.
        // This range allows the usage of an approximation that reduces computational errors.
        result = log1pl(x);
    } else {
        // For values outside the specialized range, calculate directly.
        // This involves more straightforward computation using the standard log function,
        // which is accurate enough outside the critical range around zero.
        result = logl(1.0L + x);
    }

    return result;
}
