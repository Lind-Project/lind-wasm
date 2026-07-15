/* Set floating-point exception flags.  Software-simulated edition --
   see fenv_libc.h.  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__fesetexceptflag (const fexcept_t *flagp, int excepts)
{
  excepts &= FE_ALL_EXCEPT;
  __lind_fe_exceptions = (__lind_fe_exceptions & ~excepts) | (*flagp & excepts);
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__fesetexceptflag, __old_fesetexceptflag)
compat_symbol (libm, __old_fesetexceptflag, fesetexceptflag, GLIBC_2_1);
#endif
versioned_symbol (libm, __fesetexceptflag, fesetexceptflag, GLIBC_2_2);
