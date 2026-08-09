/* Minimal reproducer for test-ldouble-lgamma: REAL_FAIL in baseline
 * (genuine numeric bug already, separate from interposition), but under
 * strict libm interposition it regresses further to CRASH -- traps before
 * printing anything past glibc's own test-harness header, RC=1.
 *
 * Suspected cause: same long-double/fp128 ABI marshalling gap as
 * libm_interpose_repro_fmodl.c (see
 * issues/fix-complex-and-ldbl-abi-marshalling.md).
 *
 * Run baseline:
 *   scripts/lind_compile tests/playground/libm_interpose_repro_lgammal.c -lm
 *   cp tests/playground/libm_interpose_repro_lgammal.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_lgammal.wasm
 *
 * Run under strict interposition (expected to crash before the second printf):
 *   LIND_LIBM_MODE=interposed scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_lgammal.wasm
 */
#include <math.h>
#include <stdio.h>

int main(void) {
  printf("calling lgammal...\n");
  long double r = lgammal(5.0L);
  printf("lgammal(5.0) = %Lg (expected ~3.178)\n", r);
  return 0;
}
