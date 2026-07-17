/* Define iscanonical macro -- lind (ldbl-128) override.

   sysdeps/ieee754/ldbl-128 (this port's long-double implementation, see
   sysdeps/lind/Implies) has no bits/iscanonical.h of its own. Header
   resolution falls through, past ldbl-128, to sysdeps/ieee754/ldbl-96's
   override (still present in the resolved sysdeps chain as a low-priority
   fallback via sysdeps/i386/Implies, since this port configures with
   --host=i686-linux-gnu -- see sysdeps/lind/Makeconfig for the same
   shadowing pattern with float128-fcts). ldbl-96's iscanonical() is a real
   function call to __iscanonicall(), needed because x87 80-bit extended
   precision has genuine non-canonical bit patterns (pseudo-denormals /
   unnormals). No such function exists for plain ldbl-128 (nothing provides
   __iscanonicall for it -- math/s_iscanonicall.c, the generic stub, is
   deliberately empty: "Not needed by default"), so pulling in ldbl-96's
   version leaves __iscanonicall undefined at link time.

   Standard IEEE-754 binary128 has no non-canonical encodings at all, so the
   correct behavior here is the fully-generic bits/iscanonical.h definition
   (repo root) -- reproduced here, at sysdeps/lind's higher search priority,
   so it's picked up before ldbl-96's override rather than after. */

#ifndef _MATH_H
# error "Never use <bits/iscanonical.h> directly; include <math.h> instead."
#endif

#define iscanonical(x) ((void) (__typeof (x)) (x), 1)
