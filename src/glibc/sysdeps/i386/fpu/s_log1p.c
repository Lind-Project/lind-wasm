#include <math.h>

double __log1p(double x) {
    // Constants to define the limit for using fyl2xp1 approximation.
    const double limit = 0.29;
    //const double one = 1.0;
    double result;

    if (isnan(x) || isinf(x)) {
        // Handle special cases directly:
        // log1p(Inf) = Inf, log1p(-Inf) = NaN (log of a negative number is not defined)
        result = (x == -INFINITY) ? NAN : x;
    } else if (fabs(x) <= limit) {
        // Use fyl2xp1 for x in range -0.29 to 0.29.
        // fyl2xp1(x) computes (x * log2(e)) + log(1 + x) for small x
        // Conversion to base e logarithm: log_b(x) = log_c(x) / log_c(b)
        result = log1p(x);
    } else {
        // For values outside -0.29 to 0.29 range, we use the direct computation.
        // log(1 + x) directly for better accuracy outside the limit.
        result = log(1.0 + x);
    }

    // This assumes handling of underflow behavior to be compliant with the standard library.
    return result;
}

