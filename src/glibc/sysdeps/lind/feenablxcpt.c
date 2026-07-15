/* Enable floating-point exception traps.  Software-simulated edition --
   see fenv_libc.h.  Only affects exceptions later raised through
   feraiseexcept()/feupdateenv() -- there is no automatic hardware trap
   on wasm32 to unmask.  */

#include "fenv_libc.h"

int
feenableexcept (int excepts)
{
  int old_enabled = ~__lind_fe_disabled & FE_ALL_EXCEPT;
  __lind_fe_disabled &= ~(excepts & FE_ALL_EXCEPT);
  return old_enabled;
}
