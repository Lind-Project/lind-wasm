#include <math.h>  // Include the math library for scalbnl()

long double __scalbnl(long double x, int n) {
    return scalbnl(x, n);  // Use the standard library function to scale by powers of 2
}

