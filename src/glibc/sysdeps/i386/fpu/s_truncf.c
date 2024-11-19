
//#include <libm-alias-float.h>
#include <math.h>  // Include the math library for truncf()

float Mytruncf(float x) {
    return truncf(x);  // Use the standard library function truncf()
}

// This definition is typically unnecessary if your environment
// provides a compliant C standard library, which would include truncf() by default.

//libm_alias_float (__trunc, trunc)
