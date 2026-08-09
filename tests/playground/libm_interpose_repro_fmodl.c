/* Minimal reproducer for test-ldouble-fmod: FLAG_ONLY_EXCEPTION in
 * baseline (numerically correct, only exception-flag checks differ -- a
 * pre-existing, separate gap), but under strict libm interposition it
 * regresses all the way to CRASH: the process traps before printing
 * anything at all, RC=1.
 *
 * Bisection notes (a single fmodl() call does NOT reproduce this, whether
 * with fixed or "large" arguments -- verified both ways):
 *  - A fixed-value loop (`fmodl(5.3L, 2.0L)` repeated 50x) does NOT crash.
 *  - A single call with any tested value, including large ones like
 *    fmodl(54.3L, 2.0L) or the IEEE-754 edge cases (0, -0, inf, x%0),
 *    does NOT crash on its own.
 *  - A loop over *varying* long double values (`fmodl(5.3L + i, 2.0L)`)
 *    reproduces reliably once the loop bound reaches ~20 iterations (15
 *    iterations completes fine; 20 traps with zero output, not even the
 *    first iteration's printf). That "the trip count itself changes
 *    whether the very first iteration succeeds" points at a compile-time
 *    stack-layout effect (see the `wasm trap: call stack exhausted`
 *    already seen elsewhere for this port's fp128/long-double handling),
 *    not a runtime resource leak -- most likely the combination of
 *    long-double's 16-byte ABI footprint plus per-call marshalling
 *    overhead tips an already-tight stack budget over the edge only for
 *    certain codegen (loop unrolling/vectorization) choices, which
 *    baseline's direct (unmarshalled) call path has enough headroom to
 *    avoid.
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_fmodl.c -lm
 *   cp tests/playground/libm_interpose_repro_fmodl.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_fmodl.wasm
 *
 * Run under strict interposition (expected: zero output, RC=1):
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_fmodl.wasm
 */
#include <math.h>
#include <stdio.h>

int main(void) {
  for (int i = 0; i < 20; i++) {
    long double r = fmodl(5.3L + i, 2.0L);
    printf("iter %d: fmodl(%d, 2.0) = %Lg\n", i, i, r);
  }
  printf("done\n");
  return 0;
}
