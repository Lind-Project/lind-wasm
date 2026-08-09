/* Minimal reproducer for test-ldouble-remquo: FLAG_ONLY_EXCEPTION in
 * baseline (numerically correct, only exception-flag checks differ -- a
 * pre-existing, separate gap), but under strict libm interposition it
 * regresses all the way to CRASH -- traps before printing anything past
 * glibc's own test-harness header, RC=1.
 *
 * Suspected cause: same long-double/fp128 ABI marshalling gap as
 * libm_interpose_repro_fmodl.c (see
 * issues/fix-complex-and-ldbl-abi-marshalling.md) -- remquol also has an
 * `int *quo` out-pointer argument in addition to two `long double` args,
 * so it exercises both the fp128 ABI gap and pointer-shadow marshalling
 * together.
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_remquol.c -lm
 *   cp tests/playground/libm_interpose_repro_remquol.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_remquol.wasm
 *
 * Run under strict interposition (expected to crash before the second printf):
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_remquol.wasm
 */
#include <math.h>
#include <stdio.h>

int main(void) {
  printf("calling remquol...\n");
  int quo = 0;
  long double r = remquol(5.3L, 2.0L, &quo);
  /* remquo rounds to the nearest multiple of 2.0 (3), unlike fmod's
   * truncation -- 5.3 - 3*2.0 = -0.7 is the correct result, not 1.3. */
  printf("remquol(5.3, 2.0, &quo) = %Lg, quo = %d (expected -0.7, 3)\n", r, quo);
  return 0;
}
