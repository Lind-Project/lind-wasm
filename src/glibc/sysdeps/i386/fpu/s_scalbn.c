#include <math.h>  // Include the math library for scalbn()

double __scalbn(double x, int n) {
    return scalbn(x, n);  // Use the standard library function to scale by powers of 2
}

