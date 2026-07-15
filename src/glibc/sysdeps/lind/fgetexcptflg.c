/* Store current representation for exceptions.  Software-simulated
   edition -- see fenv_libc.h.  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__fegetexceptflag (fexcept_t *flagp, int excepts)
{
  *flagp = (fexcept_t) (__lind_fe_exceptions & excepts & FE_ALL_EXCEPT);
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__fegetexceptflag, __old_fegetexceptflag)
compat_symbol (libm, __old_fegetexceptflag, fegetexceptflag, GLIBC_2_1);
#endif
versioned_symbol (libm, __fegetexceptflag, fegetexceptflag, GLIBC_2_2);
