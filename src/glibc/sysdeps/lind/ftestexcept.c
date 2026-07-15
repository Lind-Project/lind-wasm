/* Test exception in current environment.  Software-simulated edition --
   see fenv_libc.h.  */

#include "fenv_libc.h"

int
fetestexcept (int excepts)
{
  return __lind_fe_exceptions & excepts & FE_ALL_EXCEPT;
}
libm_hidden_def (fetestexcept)
