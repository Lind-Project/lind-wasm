/* Minimal reproducer for the cos(inf)/cos(-inf) FLAG_ONLY gap seen in the glibc
 * libm test suite under lind-wasm: the numeric result is correct (NaN), but
 * errno isn't set to EDOM and FE_INVALID isn't raised, unlike on real x86 hardware.
 * No grate/interposition involved — this is baseline behavior. See run this via:
 *   scripts/lind_compile repro_cos_inf.c -o repro_cos_inf.wasm -- -lm
 *   cp repro_cos_inf.wasm lindfs/ && scripts/lind_run repro_cos_inf.wasm
 */
#include <errno.h>
#include <fenv.h>
#include <math.h>
#include <stdio.h>

static void check(double x, const char *label) {
  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double r = cos(x);
  int invalid_set = fetestexcept(FE_INVALID) != 0;
  printf("cos(%s) = %g\n", label, r);
  printf("  errno = %d (expected %d / EDOM)\n", errno, EDOM);
  printf("  FE_INVALID set = %d (expected 1)\n", invalid_set);
}

int main(void) {
  check(INFINITY, "+inf");
  check(-INFINITY, "-inf");
  return 0;
}
