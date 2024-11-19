#include <math.h>

int __finitef(float x) {
    // Use the isfinite macro from math.h to check if the number is finite
    return isfinite(x);
}

