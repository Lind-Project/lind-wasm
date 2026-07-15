/* Storage and shared helper for the software-simulated fenv.  See
   fenv_libc.h for what each variable means and why wasm32 needs this at
   all instead of the normal hardware control/status register access.  */

#include "fenv_libc.h"
#include <signal.h>

__thread int __lind_fe_exceptions;
__thread int __lind_fe_disabled = FE_ALL_EXCEPT;
__thread int __lind_fe_round_mode;

void
__lind_fe_raise (int excepts)
{
  __lind_fe_exceptions |= excepts;
  if (excepts & ~__lind_fe_disabled)
    raise (SIGFPE);
}
