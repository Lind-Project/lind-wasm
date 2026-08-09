/* Minimal reproducer for test-ldouble-pow: REAL_FAIL in baseline (the real
 * test runs to completion and finds 1713 genuine failures -- mostly
 * "Exception \"Invalid operation\" not set" for sNaN/qNaN inputs, a
 * separate pre-existing gap -- then exits 1), but under strict libm
 * interposition it regresses to CRASH: the process traps immediately,
 * before printing anything but glibc's own test-harness header.
 *
 * NOTE: powl(2.0L, 10.0L) (an exact integer exponent) is NOT a good
 * reproducer -- it crashes with "wasm trap: call stack exhausted" even in
 * BASELINE, a separate, unrelated pre-existing bug in this port's powl
 * integer-exponent path (likely unbounded recursion in repeated-squaring).
 * A plain fractional exponent avoids that and isolates the interposition-
 * specific regression cleanly.
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_powl.c -lm
 *   cp tests/playground/libm_interpose_repro_powl.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_powl.wasm
 *
 * Run under strict interposition (expected to crash before the second printf):
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_powl.wasm
 */
#include <math.h>
#include <stdio.h>

int main(void) {
  printf("calling powl...\n");
  long double r = powl(1.5L, 1.5L);
  printf("powl(1.5, 1.5) = %Lg (expected ~1.83712)\n", r);
  return 0;
}
