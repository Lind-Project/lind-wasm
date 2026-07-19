/* Internal state for the software-simulated fenv on this target.
   wasm32 has no hardware FP control/status register, so every one of
   these is plain per-thread bookkeeping: it is only ever populated where
   library code explicitly calls feraiseexcept()/fesetexcept() -- there
   is no hardware side effect to hook into, and fesetround() only ever
   actually accepts FE_TONEAREST since wasm32 arithmetic has no other
   rounding mode.  */

#ifndef _LIND_FENV_LIBC_H
#define _LIND_FENV_LIBC_H 1

#include <fenv.h>

/* Rounding-mode bits occupy the top nibble of the x86-shaped
   __control_word we reuse for storage; exception (mask) bits occupy the
   bottom byte, matching FE_ALL_EXCEPT and real x87's own control-word
   layout.  */
#define __LIND_ROUND_MASK (FE_DOWNWARD | FE_UPWARD | FE_TOWARDZERO)

/* Sticky raised-exception bits (subset of FE_ALL_EXCEPT).  */
extern __thread int __lind_fe_exceptions attribute_tls_model_ie;

/* Currently masked (trap-disabled) exceptions; 1 = masked.  Defaults to
   FE_ALL_EXCEPT (nothing traps), matching real hardware's default
   control word.  */
extern __thread int __lind_fe_disabled attribute_tls_model_ie;

/* Current rounding mode.  Always FE_TONEAREST in practice -- see
   fesetround.c -- but tracked so fegetround()/fegetenv()/fesetenv()
   round-trip consistently.  */
extern __thread int __lind_fe_round_mode attribute_tls_model_ie;

/* OR excepts into __lind_fe_exceptions and raise(SIGFPE) if any of them
   are currently unmasked.  Shared by feraiseexcept() and feupdateenv().  */
extern void __lind_fe_raise (int excepts) attribute_hidden;

#endif /* fenv_libc.h */
