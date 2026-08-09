/* Minimal reproducer for test-double-lgamma: CLEAN in baseline, but under
 * strict libm interposition shows 1320 failures -- far more than the other
 * FLAG_ONLY_EXCEPTION regressions (acos/cosh/remainder), because lgamma()
 * fails on *two* independent axes at once:
 *
 *  1. "Exception \"Divide by zero\" not set" at the pole (lgamma(0)) --
 *     same suspected worker-affinity cause as the other three reproducers
 *     in this directory (see comp_errno_propagation.md).
 *
 *  2. "extra output 1" on almost every single call -- lgamma() sets the
 *     global `signgam` variable as a side effect (the sign of the true
 *     gamma function, since lgamma itself returns log(|gamma(x)|)). This
 *     is the EXACT same shape of bug errno had before the fix in
 *     linker.rs/threei.rs/lind-3i: ambient global state written by the
 *     real function inside the grate's own address space, invisible to
 *     the caller because it's not part of lgamma's marshal spec (no
 *     explicit arg/return carries it). signgam never got the seed/relay
 *     treatment errno did, so it's *always* wrong under interposition,
 *     regardless of worker affinity -- this reproduces even on a single-
 *     worker grate.
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_lgamma.c -lm
 *   cp tests/playground/libm_interpose_repro_lgamma.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_lgamma.wasm
 *
 * Run under strict interposition:
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_lgamma.wasm
 */
#include <fenv.h>
#include <math.h>
#include <stdio.h>

extern int signgam;

int main(void) {
  feclearexcept(FE_ALL_EXCEPT);
  signgam = 0;
  double r = lgamma(0.0); /* pole at 0 -> must set FE_DIVBYZERO, signgam = 1 */
  printf("lgamma(0) = %g\n", r);
  printf("FE_DIVBYZERO %s (expected SET)\n",
         fetestexcept(FE_DIVBYZERO) != 0 ? "SET" : "NOT SET");
  printf("signgam = %d (expected 1)\n", signgam);
  return 0;
}
