/* Set floating-point environment.  Software-simulated edition -- see
   fenv_libc.h.  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__fesetenv (const fenv_t *envp)
{
  if (envp == FE_DFL_ENV)
    {
      __lind_fe_exceptions = 0;
      __lind_fe_round_mode = FE_TONEAREST;
      __lind_fe_disabled = FE_ALL_EXCEPT;
    }
#ifdef FE_NOMASK_ENV
  else if (envp == FE_NOMASK_ENV)
    {
      __lind_fe_exceptions = 0;
      __lind_fe_round_mode = FE_TONEAREST;
      __lind_fe_disabled = 0;
    }
#endif
  else
    {
      __lind_fe_exceptions = envp->__status_word & FE_ALL_EXCEPT;
      __lind_fe_round_mode = envp->__control_word & __LIND_ROUND_MASK;
      __lind_fe_disabled = envp->__control_word & FE_ALL_EXCEPT;
    }
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__fesetenv, __old_fesetenv)
compat_symbol (libm, __old_fesetenv, fesetenv, GLIBC_2_1);
#endif
libm_hidden_def (__fesetenv)
libm_hidden_ver (__fesetenv, fesetenv)
versioned_symbol (libm, __fesetenv, fesetenv, GLIBC_2_2);
