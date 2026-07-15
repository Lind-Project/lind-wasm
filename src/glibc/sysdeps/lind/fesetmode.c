/* Install given floating-point control modes.  Software-simulated
   edition -- see fenv_libc.h.  */

#include "fenv_libc.h"

int
fesetmode (const femode_t *modep)
{
  if (modep == FE_DFL_MODE)
    {
      __lind_fe_round_mode = FE_TONEAREST;
      __lind_fe_disabled = FE_ALL_EXCEPT;
    }
  else
    {
      __lind_fe_round_mode = modep->__control_word & __LIND_ROUND_MASK;
      __lind_fe_disabled = modep->__control_word & FE_ALL_EXCEPT;
    }
  return 0;
}
