/* Raise given exceptions.  Software-simulated edition -- see
   fenv_libc.h.  Only ever fires for exceptions that library code (or the
   caller) explicitly raises -- wasm32 arithmetic has no hardware trap to
   hook automatically.  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__feraiseexcept (int excepts)
{
  __lind_fe_raise (excepts & FE_ALL_EXCEPT);
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__feraiseexcept, __old_feraiseexcept)
compat_symbol (libm, __old_feraiseexcept, feraiseexcept, GLIBC_2_1);
#endif
libm_hidden_def (__feraiseexcept)
libm_hidden_ver (__feraiseexcept, feraiseexcept)
versioned_symbol (libm, __feraiseexcept, feraiseexcept, GLIBC_2_2);
