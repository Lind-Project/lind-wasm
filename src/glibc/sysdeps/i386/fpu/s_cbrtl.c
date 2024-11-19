/* Compute cubic root of long double value.
   Copyright (C) 1997-2024 Free Software Foundation, Inc.
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

#include <libm-alias-ldouble.h>
#include <math.h>
#include <float.h>

// Check if long double is greater in precision than double
#if LDBL_MANT_DIG == 53
// If so, use double precision cbrt which is usually present
long double __cbrtl(long double x) {
    return (long double)cbrt((double)x);
}
#else
// Use the dedicated long double version if available
long double __cbrtl(long double x) {
    return cbrtl(x);
}
#endif

libm_alias_ldouble (__cbrt, cbrt)
