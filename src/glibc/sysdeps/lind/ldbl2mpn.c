/* Convert a `long double' to multi-precision integer.  Lind/WASM version.

   On wasm32, long double is 128-bit IEEE quad (sizeof == 16), but glibc
   is configured with ldbl-96 (x86 80-bit extended precision).  The
   ldbl-96 __mpn_extract_long_double interprets the bits using the wrong
   layout, causing printf %f/%e/%Lf to produce NAN.

   We cannot simply cast to double because the runtime lacks __trunctfdf2
   (128-bit soft-float builtins are not linked).  Instead, we extract the
   value by reinterpreting the 128-bit IEEE quad bit pattern directly,
   pulling out the sign, exponent, and top 52 bits of the 112-bit
   mantissa to reconstruct a double, then delegate to __mpn_extract_double.

   Copyright (C) 2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.  */

#include "gmp.h"
#include "gmp-impl.h"
#include <stdint.h>
#include <string.h>

extern mp_size_t __mpn_extract_double (mp_ptr res_ptr, mp_size_t size,
                                       int *expt, int *is_neg,
                                       double value);

mp_size_t
__mpn_extract_long_double (mp_ptr res_ptr, mp_size_t size,
                           int *expt, int *is_neg,
                           long double value)
{
  /* IEEE 754 quad:  1 sign | 15 exponent | 112 mantissa
     IEEE 754 double: 1 sign | 11 exponent |  52 mantissa

     Quad bias = 16383, double bias = 1023.
     We extract sign + exponent + top 52 mantissa bits from the quad
     and reassemble them into a double bit pattern.  */

  uint64_t hi, lo;
  memcpy (&lo, &value, 8);
  memcpy (&hi, (char *)&value + 8, 8);

  uint64_t sign = (hi >> 63) & 1;
  int q_exp = (hi >> 48) & 0x7FFF;
  /* Top 48 bits of mantissa are in hi[47:0], next 64 bits in lo. */
  uint64_t q_mant_hi = hi & 0x0000FFFFFFFFFFFFULL;

  double dval;

  if (q_exp == 0x7FFF)
    {
      /* Inf or NaN — preserve class in double. */
      uint64_t d_bits = (sign << 63) | 0x7FF0000000000000ULL;
      if (q_mant_hi != 0 || lo != 0)
        d_bits |= 1;  /* NaN */
      memcpy (&dval, &d_bits, 8);
    }
  else if (q_exp == 0 && q_mant_hi == 0 && lo == 0)
    {
      /* Zero. */
      uint64_t d_bits = sign << 63;
      memcpy (&dval, &d_bits, 8);
    }
  else
    {
      /* Normal or subnormal quad value.
         Rebias exponent: double_exp = quad_exp - 16383 + 1023.  */
      int d_exp = q_exp - 16383 + 1023;

      /* Top 52 bits of quad mantissa → double mantissa.
         quad mantissa: q_mant_hi (48 bits) | lo (64 bits) = 112 bits.
         We need top 52 bits, which is all 48 bits of q_mant_hi
         plus the top 4 bits of lo.  */
      uint64_t d_mant = (q_mant_hi << 4) | (lo >> 60);

      if (d_exp <= 0)
        {
          /* Underflow to double zero. */
          uint64_t d_bits = sign << 63;
          memcpy (&dval, &d_bits, 8);
        }
      else if (d_exp >= 0x7FF)
        {
          /* Overflow to double infinity. */
          uint64_t d_bits = (sign << 63) | 0x7FF0000000000000ULL;
          memcpy (&dval, &d_bits, 8);
        }
      else
        {
          uint64_t d_bits = (sign << 63)
                            | ((uint64_t) d_exp << 52)
                            | d_mant;
          memcpy (&dval, &d_bits, 8);
        }
    }

  return __mpn_extract_double (res_ptr, size, expt, is_neg, dval);
}
