/* Store current floating-point environment and clear exceptions.
   Software-simulated edition -- see fenv_libc.h.  */

#include "fenv_libc.h"

int
__feholdexcept (fenv_t *envp)
{
  __fegetenv (envp);
  __lind_fe_exceptions = 0;
  __lind_fe_disabled = FE_ALL_EXCEPT;
  return 0;
}
libm_hidden_def (__feholdexcept)
weak_alias (__feholdexcept, feholdexcept)
libm_hidden_weak (feholdexcept)
