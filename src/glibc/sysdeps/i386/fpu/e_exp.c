/* Double-precision e^x function.
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

double __ieee754_exp(double x) {
    // Check for NaN or infinity input
    if (isnan(x) || isinf(x)) {
        if (x < 0) {
            // If x is negative infinity, exp(x) should be 0
            return 0.0;
        } else {
            // If x is positive infinity, exp(x) should be infinity
            return INFINITY;
        }
    }

    // Calculate x * log2(e)
    double x_log2e = x * M_LOG2E;  // M_LOG2E is the multiplier for change of base from e to 2
    double int_part;
    double fract_part = modf(x_log2e, &int_part); // Split x * log2(e) into integer and fractional parts

    // Calculate 2^(fract(x * log2(e)))
    double exp_fract = exp2(fract_part); // exp2 computes 2 raised to the given power

    // Scale by 2^(int(x * log2(e))) equivalent to multiplying by 2 raised to the int part
    return ldexp(exp_fract, (int)int_part);  // ldexp function multiplies exp_fract by 2 raised to the power of int_part
}

double __exp_finite(double x) {
    return __ieee754_exp(x);  // Alias for __ieee754_exp as no special handling for finite range in this context
}

libm_alias_finite (__ieee754_exp, __exp)
