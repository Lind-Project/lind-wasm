/* Wrapper to set errno for exp2.
   Copyright (C) 2017-2024 Free Software Foundation, Inc.
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

/* Only build wrappers from the templates for the types that define the macro
   below.  This macro is set in math-type-macros-<type>.h in sysdeps/generic
   for each floating-point type.  */
#if __USE_WRAPPER_TEMPLATE

#include <math.h>
#include <math_private.h>
#include <math-underflow.h>
#include <float.h>

FLOAT
M_DECL_FUNC (__exp2) (FLOAT x)
{

  if (__glibc_likely (isless (x, (FLOAT) M_MAX_EXP)))
    {
      if (__builtin_expect (isgreaterequal (x, (FLOAT) (M_MIN_EXP - M_MANT_DIG
							- 1)), 1))
	{
	  int intx = (int) x;
	  FLOAT fractx = x - intx;
	  FLOAT result;
	  if (M_FABS (fractx) < M_EPSILON / 4)
	    result = M_SCALBN (1 + fractx, intx);
	  else
	    result = M_SCALBN (M_EXP (M_MLIT (M_LN2) * fractx), intx);
	  math_check_force_underflow_nonneg (result);
	  return result;
	}
      else
	{
	  /* Underflow or exact zero.  */
	  if (isinf (x))
	    return 0;
	  else
	    return M_MIN * M_MIN;
	}
    }
  else
    /* Infinity, NaN or overflow.  */
    return M_MAX * x;
}
declare_mgen_alias (__exp2, exp2)

#endif /* __USE_WRAPPER_TEMPLATE.  */
