
//#include <libm-alias-float.h>
#include <math.h>  // Include the math library for rintf()

float Myrintf(float x) {
    return rintf(x);  // Use the standard library function to round to the nearest integer
}

//libm_alias_float (__rint, rint)
