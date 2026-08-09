/* Minimal reproducer for test-double-acos: CLEAN in baseline, but under
 * strict libm interposition (LIND_LIBM_MODE=interposed + libm_full_grate)
 * shows "Exception \"Invalid operation\" not set" for out-of-domain acos
 * inputs (e.g. acos(1.125), |x| > 1).
 *
 * feclearexcept/fetestexcept are themselves interposed (real libm exports),
 * so if they and acos() don't land on the SAME grate worker, the flag one
 * worker set is invisible to a different worker's fetestexcept() read --
 * see comp_errno_propagation.md's worker-affinity hypothesis (the software
 * fenv state has the same ambient-TLS shape as errno, but no seed/relay fix
 * was ever added for it).
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_acos.c -lm
 *   cp tests/playground/libm_interpose_repro_acos.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_acos.wasm
 *
 * Run under strict interposition:
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_acos.wasm
 */
#include <fenv.h>
#include <math.h>
#include <stdio.h>

int main(void) {
  feclearexcept(FE_ALL_EXCEPT);
  double r = acos(1.125); /* |x| > 1 -> domain error, must set FE_INVALID */
  printf("acos(1.125) = %g\n", r);
  printf("FE_INVALID %s (expected SET)\n",
         fetestexcept(FE_INVALID) != 0 ? "SET" : "NOT SET");
  return 0;
}
