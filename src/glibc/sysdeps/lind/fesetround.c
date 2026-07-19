/* Set rounding mode.  Software-simulated edition -- see fenv_libc.h.

   wasm32 arithmetic is hardwired to round-to-nearest; there is no ISA
   hook to make it round any other way, so anything other than
   FE_TONEAREST is honestly reported as unsupported (nonzero return)
   rather than silently tracked-but-ignored.  */

#include "fenv_libc.h"

int
__fesetround (int round)
{
  if (round != FE_TONEAREST)
    return 1;

  __lind_fe_round_mode = round;
  return 0;
}
libm_hidden_def (__fesetround)
weak_alias (__fesetround, fesetround)
libm_hidden_weak (fesetround)
