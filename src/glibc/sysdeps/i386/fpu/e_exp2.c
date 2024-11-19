/* Double-precision 2^x function.
   Copyright (C) 2018-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   The GNU C Library is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 2.1 of the License, or (at your option) any later version.

   The GNU C Library is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <math.h>
#include <math_private.h>
#include <libm-alias-finite.h>
#include <float.h>

#include <math.h>
#include <float.h>

double __ieee754_exp2(double x) {
    // Check if the input x is NaN or infinity
    if (isnan(x) || isinf(x)) {
        if (x < 0) {
            // If x is negative infinity, exp2(x) should return 0
            return 0.0;
        } else {
            // If x is positive infinity, exp2(x) should return positive infinity
            return INFINITY;
        }
    }

    // Calculation for normal cases
    double int_part;
    double fract_part = modf(x, &int_part); // Split x into integer and fractional parts
    double exp2_fract = exp2(fract_part);   // 2 raised to the fractional part
    return ldexp(exp2_fract, (int)int_part);  // Scale the result by 2 raised to the integer part
}

libm_alias_finite (__ieee754_exp2, __exp2)
