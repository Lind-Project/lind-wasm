/* Test whether long double value is canonical -- lind (ldbl-128) version.

   Standard IEEE-754 binary128 (this port's long-double format, see
   sysdeps/lind/Implies) has no non-canonical encodings, unlike x87 80-bit
   extended precision (ldbl-96, pseudo-denormals/unnormals) or IBM
   double-double (ldbl-128ibm) -- matching sysdeps/lind/bits/iscanonical.h's
   own macro fast-path, which always returns true for this reason. That macro
   is what glibc's own internal code (e.g. s_canonicalizel.c) actually calls;
   this real function exists only so the __iscanonicall symbol itself is
   genuinely defined -- needed for anything (e.g. an interposition grate)
   that references it as an actual exported symbol rather than inlining the
   macro. */

#include <math.h>

int
__iscanonicall (long double x)
{
  (void) x;
  return 1;
}
libm_hidden_def (__iscanonicall)
