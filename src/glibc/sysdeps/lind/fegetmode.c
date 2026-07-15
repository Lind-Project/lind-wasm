/* Store current floating-point control modes.  Software-simulated
   edition -- see fenv_libc.h.  */

#include "fenv_libc.h"

int
fegetmode (femode_t *modep)
{
  modep->__control_word = (__lind_fe_round_mode & __LIND_ROUND_MASK)
			   | (__lind_fe_disabled & FE_ALL_EXCEPT);
  modep->__glibc_reserved = 0;
  modep->__mxcsr = 0;
  return 0;
}
