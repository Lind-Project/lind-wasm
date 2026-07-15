/* Store current floating-point environment.  Software-simulated edition
   -- see fenv_libc.h.  __status_word/__control_word are reused purely as
   storage slots (no real x87 register behind them).  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__fegetenv (fenv_t *envp)
{
  envp->__status_word = __lind_fe_exceptions & FE_ALL_EXCEPT;
  envp->__control_word = (__lind_fe_round_mode & __LIND_ROUND_MASK)
			  | (__lind_fe_disabled & FE_ALL_EXCEPT);
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__fegetenv, __old_fegetenv)
compat_symbol (libm, __old_fegetenv, fegetenv, GLIBC_2_1);
#endif
libm_hidden_def (__fegetenv)
libm_hidden_ver (__fegetenv, fegetenv)
versioned_symbol (libm, __fegetenv, fegetenv, GLIBC_2_2);
