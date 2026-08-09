/* Minimal reproducer for test-float-remainder: CLEAN in baseline, but under
 * strict libm interposition shows "Exception \"Invalid operation\" not set"
 * for remainder(x, 0) -- an IEEE-754 invalid operation (undefined result).
 *
 * Same suspected root cause as libm_interpose_repro_acos.c (worker-affinity
 * for the software fenv state, see comp_errno_propagation.md).
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_remainder.c -lm
 *   cp tests/playground/libm_interpose_repro_remainder.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_remainder.wasm
 *
 * Run under strict interposition:
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_remainder.wasm
 */
#include <fenv.h>
#include <math.h>
#include <stdio.h>

int main(void) {
  feclearexcept(FE_ALL_EXCEPT);
  float r = remainderf(1.0f, 0.0f); /* divisor 0 -> invalid, must set FE_INVALID */
  printf("remainderf(1, 0) = %g\n", (double)r);
  printf("FE_INVALID %s (expected SET)\n",
         fetestexcept(FE_INVALID) != 0 ? "SET" : "NOT SET");
  return 0;
}
