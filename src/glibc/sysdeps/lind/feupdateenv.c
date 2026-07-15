/* Install given floating-point environment and raise exceptions that
   were pending before the switch.  Software-simulated edition -- see
   fenv_libc.h.  */

#include "fenv_libc.h"
#include <shlib-compat.h>

int
__feupdateenv (const fenv_t *envp)
{
  int saved_exceptions = __lind_fe_exceptions;
  __fesetenv (envp);
  __lind_fe_raise (saved_exceptions);
  return 0;
}
#if SHLIB_COMPAT (libm, GLIBC_2_1, GLIBC_2_2)
strong_alias (__feupdateenv, __old_feupdateenv)
compat_symbol (libm, __old_feupdateenv, feupdateenv, GLIBC_2_1);
#endif
libm_hidden_def (__feupdateenv)
libm_hidden_ver (__feupdateenv, feupdateenv)
versioned_symbol (libm, __feupdateenv, feupdateenv, GLIBC_2_2);
