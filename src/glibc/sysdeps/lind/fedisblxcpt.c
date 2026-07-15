/* Disable floating-point exception traps.  Software-simulated edition --
   see fenv_libc.h.  */

#include "fenv_libc.h"

int
fedisableexcept (int excepts)
{
  int old_enabled = ~__lind_fe_disabled & FE_ALL_EXCEPT;
  __lind_fe_disabled |= (excepts & FE_ALL_EXCEPT);
  return old_enabled;
}
