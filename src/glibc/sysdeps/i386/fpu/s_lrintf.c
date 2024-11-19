#include <math.h>  // Include for lrintf()
//#include <libm-alias-float.h>

float Mylrintf(float x) {
    // Convert float to nearest integer using the current rounding mode.
    return (float)lrintf(x);
}

//libm_alias_float(__lrintf, lrint);

