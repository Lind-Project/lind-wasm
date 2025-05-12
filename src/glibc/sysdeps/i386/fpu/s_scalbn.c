// #include <math.h>  // Include the math library for scalbn()

double __scalbn(double x, int n) {
    // return scalbn(x, n);  // Use the standard library function to scale by powers of 2

    // Handle special case where x is zero
    if (x == 0.0) {
        return 0.0;
    }

    // Decompose the double into its components: sign, exponent, and mantissa
    union {
        double d;
        struct {
            unsigned long long mantissa : 52;
            unsigned int exponent : 11;
            unsigned int sign : 1;
        } parts;
    } u;

    u.d = x;

    // IEEE 754 bias for double precision floating point
    int bias = 1023;

    // Calculate the new exponent
    int new_exp = (int)u.parts.exponent + n;

    if (new_exp < 1) {
        // Underflow, return 0.0 with the same sign as x
        return 0.0;
    } else {
        // Set the new exponent
        u.parts.exponent = new_exp;
        return u.d;
    }
}

