/* Set given exception flags without trapping.  Software-simulated
   edition -- see fenv_libc.h.  Unlike feraiseexcept, this never raises
   SIGFPE even if the exception is unmasked.  */

#include "fenv_libc.h"

int
fesetexcept (int excepts)
{
  __lind_fe_exceptions |= (excepts & FE_ALL_EXCEPT);
  return 0;
}
