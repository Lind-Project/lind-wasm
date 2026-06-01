/* Convert a `long double' to multi-precision integer.  Lind/WASM version.

   On wasm32, long double is 128-bit IEEE quad (sizeof == 16), but glibc
   is configured with ldbl-96 (x86 80-bit extended precision).  The
   ldbl-96 __mpn_extract_long_double interprets the bits using the wrong
   layout, causing printf %f/%e/%Lf to produce NAN.

   This file provides a proper 128-bit IEEE quad extractor using raw bit
   manipulation, avoiding both the ldbl-96 ieee854_long_double union
   (wrong layout) and soft-float builtins like __trunctfdf2 (not linked).

   The logic mirrors sysdeps/ieee754/ldbl-128/ldbl2mpn.c but uses memcpy
   and uint64_t instead of the ieee854_long_double union.

   Copyright (C) 2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.  */

#include "gmp.h"
#include "gmp-impl.h"
#include "longlong.h"
#include <float.h>
#include <stdint.h>
#include <string.h>

/* IEEE 754 quad-precision layout (little-endian):
   bytes  0-7  (lo): mantissa bits [63:0]   (low 64 of 112-bit mantissa)
   bytes  8-15 (hi): sign[63] | exponent[62:48] | mantissa[47:0] (high 48)

   Total mantissa: 112 bits (implicit leading 1 for normals).
   Exponent bias: 16383.  */

#define QUAD_BIAS 16383

mp_size_t
__mpn_extract_long_double (mp_ptr res_ptr, mp_size_t size,
                           int *expt, int *is_neg,
                           long double value)
{
  uint64_t lo, hi;
  memcpy (&lo, &value, 8);
  memcpy (&hi, (char *)&value + 8, 8);

  *is_neg = (hi >> 63) & 1;
  *expt = (int)((hi >> 48) & 0x7FFF) - QUAD_BIAS;

  /* Extract 112-bit mantissa into 32-bit limbs.
     mantissa3 = lo[31:0], mantissa2 = lo[63:32],
     mantissa1 = hi[15:0]<<16 | lo... wait, no:
     hi[47:0] are the top 48 bits of mantissa,
     lo[63:0] are the low 64 bits.
     Total = 48 + 64 = 112 bits.

     For BITS_PER_MP_LIMB == 32, we need 4 limbs:
       res_ptr[0] = mantissa bits [31:0]   = lo & 0xFFFFFFFF
       res_ptr[1] = mantissa bits [63:32]  = lo >> 32
       res_ptr[2] = mantissa bits [95:64]  = hi & 0xFFFF (16 bits) | ...
     Actually, let's be more careful:
       bits [31:0]   = (uint32_t) lo
       bits [63:32]  = (uint32_t)(lo >> 32)
       bits [95:64]  = (uint32_t) (hi & 0x0000FFFFFFFFFFFF)  -- bottom 32 of hi's mantissa
       bits [111:96] = (uint32_t)((hi >> 32) & 0xFFFF)       -- top 16 of hi's mantissa
     Wait, the hi mantissa is hi[47:0] = 48 bits.
       bits [95:64]  = (uint32_t)(hi & 0xFFFFFFFF)  -- note: only bottom 32 of hi[47:0]
       Actually hi contains sign+exponent+mantissa:
       hi = [sign(1)][exponent(15)][mantissa_hi(48)]
       mantissa from hi = hi & 0x0000FFFFFFFFFFFF (48 bits)

     48 + 64 = 112 mantissa bits, packed into 4x32 = 128 bit slots:
       res_ptr[0] = lo bits [31:0]
       res_ptr[1] = lo bits [63:32]
       res_ptr[2] = hi_mant bits [31:0]   (bits 64-95 of mantissa)
       res_ptr[3] = hi_mant bits [47:32]  (bits 96-111, only 16 bits used)
  */

#if BITS_PER_MP_LIMB == 32
  uint64_t hi_mant = hi & 0x0000FFFFFFFFFFFFULL;
  res_ptr[0] = (uint32_t) lo;
  res_ptr[1] = (uint32_t)(lo >> 32);
  res_ptr[2] = (uint32_t) hi_mant;
  res_ptr[3] = (uint32_t)(hi_mant >> 32);
  #define N 4
#elif BITS_PER_MP_LIMB == 64
  res_ptr[0] = lo;
  res_ptr[1] = hi & 0x0000FFFFFFFFFFFFULL;
  #define N 2
#else
  #error "mp_limb size " BITS_PER_MP_LIMB "not accounted for"
#endif

#define NUM_LEADING_ZEROS (BITS_PER_MP_LIMB \
                           - (LDBL_MANT_DIG - ((N - 1) * BITS_PER_MP_LIMB)))

  if (((hi >> 48) & 0x7FFF) == 0)
    {
      /* Biased exponent is zero: either zero or denormal.  */
      if (res_ptr[0] == 0 && res_ptr[1] == 0
          && res_ptr[N - 2] == 0 && res_ptr[N - 1] == 0)
        *expt = 0;
      else
        {
          /* Denormal.  Normalize the mantissa.  */
          int cnt;

#if N == 2
          if (res_ptr[N - 1] != 0)
            {
              count_leading_zeros (cnt, res_ptr[N - 1]);
              cnt -= NUM_LEADING_ZEROS;
              res_ptr[N - 1] = res_ptr[N - 1] << cnt
                               | (res_ptr[0] >> (BITS_PER_MP_LIMB - cnt));
              res_ptr[0] <<= cnt;
              *expt = LDBL_MIN_EXP - 1 - cnt;
            }
          else
            {
              count_leading_zeros (cnt, res_ptr[0]);
              if (cnt >= NUM_LEADING_ZEROS)
                {
                  res_ptr[N - 1] = res_ptr[0] << (cnt - NUM_LEADING_ZEROS);
                  res_ptr[0] = 0;
                }
              else
                {
                  res_ptr[N - 1] = res_ptr[0] >> (NUM_LEADING_ZEROS - cnt);
                  res_ptr[0] <<= BITS_PER_MP_LIMB - (NUM_LEADING_ZEROS - cnt);
                }
              *expt = LDBL_MIN_EXP - 1
                      - (BITS_PER_MP_LIMB - NUM_LEADING_ZEROS) - cnt;
            }
#else
          int j, k, l;

          for (j = N - 1; j > 0; j--)
            if (res_ptr[j] != 0)
              break;

          count_leading_zeros (cnt, res_ptr[j]);
          cnt -= NUM_LEADING_ZEROS;
          l = N - 1 - j;
          if (cnt < 0)
            {
              cnt += BITS_PER_MP_LIMB;
              l--;
            }
          if (!cnt)
            for (k = N - 1; k >= l; k--)
              res_ptr[k] = res_ptr[k-l];
          else
            {
              for (k = N - 1; k > l; k--)
                res_ptr[k] = res_ptr[k-l] << cnt
                             | res_ptr[k-l-1] >> (BITS_PER_MP_LIMB - cnt);
              res_ptr[k--] = res_ptr[0] << cnt;
            }

          for (; k >= 0; k--)
            res_ptr[k] = 0;
          *expt = LDBL_MIN_EXP - 1 - l * BITS_PER_MP_LIMB - cnt;
#endif
        }
    }
  else
    /* Add the implicit leading one bit for a normalized number.  */
    res_ptr[N - 1] |= (mp_limb_t) 1 << (LDBL_MANT_DIG - 1
                                         - ((N - 1) * BITS_PER_MP_LIMB));

  return N;
}
