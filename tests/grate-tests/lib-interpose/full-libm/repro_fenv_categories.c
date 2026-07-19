/* Standalone diagnostic covering all five IEEE-754 exception categories
 * (Invalid, DivByZero, Overflow, Underflow, Inexact), each triggered via a
 * well-known operation that raises exactly that flag on real hardware.
 *
 * Unlike trig_inf_domain_error.c (which asserts and aborts on failure),
 * this one reports pass/fail for every category and keeps going, so a
 * single run shows the full picture -- useful for seeing exactly which
 * categories the software-simulated fenv (see fenv.patch /
 * sysdeps/lind/fenv_libc.h) currently covers versus not.
 *
 * GOTCHA (found while writing this -- NOT fully root-caused, flagged for
 * further investigation): printing results through a small helper function
 * (`report(category, expr, result, flag_set)`, with `fetestexcept()`
 * evaluated inline as an argument *before* the call, still in the same
 * function activation as feclearexcept()/the triggering call) made every
 * single check silently read back NOT SET, including cos(inf), which is
 * proven reliable via a direct/inline-printf structure. Swapping the
 * *exact same* 6-call sequence from a helper function to plain inline
 * printf() calls (as below) made it reliable again -- isolated via
 * bisection, not guessed. Root cause unknown; plausibly a codegen/TLS-
 * access-scheduling interaction specific to certain call shapes on this
 * target, not a logic bug in the fenv simulation itself (the *same*
 * checks, same order, same inputs, differ only in whether printing goes
 * through an extra function call). Each check below is therefore plain,
 * flat, inline code -- no helper function wrapping the check+report step.
 *
 * Run: scripts/lind_compile repro_fenv_categories.c -lm
 *      cp repro_fenv_categories.wasm lindfs/ && scripts/lind_run repro_fenv_categories.wasm
 */
#include <errno.h>
#include <fenv.h>
#include <math.h>
#include <stdio.h>

int main(void) {
  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double invalid_r = sqrt(-1.0);
  printf("Invalid      sqrt(-1.0)               = %-14g flag %s\n", invalid_r,
         fetestexcept(FE_INVALID) != 0 ? "SET" : "NOT SET (expected set)");

  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double divzero_r = log(0.0);
  printf("DivByZero    log(0.0)                 = %-14g flag %s\n", divzero_r,
         fetestexcept(FE_DIVBYZERO) != 0 ? "SET" : "NOT SET (expected set)");

  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double overflow_r = exp(1000.0);
  printf("Overflow     exp(1000.0)              = %-14g flag %s\n", overflow_r,
         fetestexcept(FE_OVERFLOW) != 0 ? "SET" : "NOT SET (expected set)");

  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double underflow_r = exp(-1000.0);
  printf("Underflow    exp(-1000.0)             = %-14g flag %s\n",
         underflow_r,
         fetestexcept(FE_UNDERFLOW) != 0 ? "SET" : "NOT SET (expected set)");

  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double inexact_r = 1.0 / 3.0;
  printf("Inexact      1.0/3.0                  = %-14g flag %s\n", inexact_r,
         fetestexcept(FE_INEXACT) != 0 ? "SET" : "NOT SET (expected set)");

  /* Contrast: cos's own domain-error branch DOES call feraiseexcept
   * explicitly (see fenv.patch) -- unlike sqrt/log/exp above, which
   * don't. Same "Invalid" category, different function, different
   * result, since the fix is scoped to cos/sin specifically. */
  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double cos_inf_r = cos(INFINITY);
  printf("Invalid*     cos(inf) [patched]       = %-14g flag %s\n", cos_inf_r,
         fetestexcept(FE_INVALID) != 0 ? "SET" : "NOT SET (expected set)");

  return 0;
}
