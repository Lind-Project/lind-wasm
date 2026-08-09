/* Minimal reproducer for test-float-cosh: CLEAN in baseline, but under
 * strict libm interposition shows "Exception \"Overflow\" not set" for
 * inputs large enough that cosh() overflows float range.
 *
 * Same suspected root cause as libm_interpose_repro_acos.c: feclearexcept/
 * fetestexcept/coshf are all separately-dispatched interposed calls that
 * must land on the same grate worker to stay consistent, and nothing
 * enforces that (unlike errno, which now has an explicit seed/relay fix --
 * see comp_errno_propagation.md).
 *
 * NOTE: the specific input value matters -- see the comment at the coshf()
 * call below.
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_cosh.c -lm
 *   cp tests/playground/libm_interpose_repro_cosh.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_cosh.wasm
 *
 * Run under strict interposition:
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_cosh.wasm
 */
#include <fenv.h>
#include <math.h>
#include <stdio.h>

int main(void) {
  feclearexcept(FE_ALL_EXCEPT);
  /* Exact value from the real glibc test (test-float-cosh.c) -- verified
   * necessary: a "simpler" overflowing input like coshf(100.0f) does NOT
   * reproduce this (it shows FE_OVERFLOW unset even in baseline, a
   * separate, pre-existing gap unrelated to interposition -- this specific
   * magnitude/code path is the one that correctly sets the flag locally). */
  float r = coshf(-0x2.c5d374p+12f);
  printf("coshf(-0x2.c5d374p+12) = %g\n", (double)r);
  printf("FE_OVERFLOW %s (expected SET)\n",
         fetestexcept(FE_OVERFLOW) != 0 ? "SET" : "NOT SET");
  return 0;
}
