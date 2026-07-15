/* Return current rounding mode.  Software-simulated edition -- see
   fenv_libc.h.  */

#include "fenv_libc.h"

int
__fegetround (void)
{
  return __lind_fe_round_mode;
}
libm_hidden_def (__fegetround)
weak_alias (__fegetround, fegetround)
libm_hidden_weak (fegetround)
