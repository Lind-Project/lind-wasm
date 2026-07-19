/* Clear given exceptions in current floating-point environment.
   Software-simulated edition -- see fenv_libc.h.  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__feclearexcept (int excepts)
{
  __lind_fe_exceptions &= ~(excepts & FE_ALL_EXCEPT);
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__feclearexcept, __old_feclearexcept)
compat_symbol (libm, __old_feclearexcept, feclearexcept, GLIBC_2_1);
#endif
libm_hidden_ver (__feclearexcept, feclearexcept)
versioned_symbol (libm, __feclearexcept, feclearexcept, GLIBC_2_2);
