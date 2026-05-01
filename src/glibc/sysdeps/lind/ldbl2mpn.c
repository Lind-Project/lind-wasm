/* Convert a `long double' to multi-precision integer.  Lind/WASM version.

   On wasm32, long double is 128-bit IEEE quad (sizeof == 16), but glibc
   is configured with ldbl-96 (x86 80-bit extended precision).  The
   ldbl-96 __mpn_extract_long_double interprets the bits using the wrong
   layout, causing printf %f/%e/%Lf to produce NAN.

   WASM has no hardware 128-bit float — all float operations happen in
   double precision.  So we delegate to __mpn_extract_double which
   handles the 64-bit IEEE754 format correctly.

   Copyright (C) 2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.  */

#include "gmp.h"
#include "gmp-impl.h"

extern mp_size_t __mpn_extract_double (mp_ptr res_ptr, mp_size_t size,
                                       int *expt, int *is_neg,
                                       double value);

mp_size_t
__mpn_extract_long_double (mp_ptr res_ptr, mp_size_t size,
                           int *expt, int *is_neg,
                           long double value)
{
  return __mpn_extract_double (res_ptr, size, expt, is_neg, (double) value);
}
