/* Get currently trap-enabled exceptions.  Software-simulated edition --
   see fenv_libc.h.  */

#include "fenv_libc.h"

int
fegetexcept (void)
{
  return ~__lind_fe_disabled & FE_ALL_EXCEPT;
}
