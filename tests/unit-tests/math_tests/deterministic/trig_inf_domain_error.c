/* cos(inf)/sin(inf) are domain errors: POSIX requires errno set to EDOM
 * and the FE_INVALID exception flag raised, in addition to returning
 * NaN.  Regression test for a lind-wasm bug where the NaN result was
 * correct but errno and FE_INVALID were silently never set.
 */
#include <assert.h>
#include <errno.h>
#include <fenv.h>
#include <math.h>
#include <stdio.h>

static void check(double (*fn)(double), const char *name, double x,
		   const char *label) {
  feclearexcept(FE_ALL_EXCEPT);
  errno = 0;
  double r = fn(x);
  int invalid_set = fetestexcept(FE_INVALID) != 0;

  printf("%s(%s) = %g\n", name, label, r);
  printf("  errno = %d (expected %d / EDOM)\n", errno, EDOM);
  printf("  FE_INVALID set = %d (expected 1)\n", invalid_set);

  assert(isnan(r));
  assert(errno == EDOM);
  assert(invalid_set == 1);
}

int main(void) {
  check(cos, "cos", INFINITY, "+inf");
  check(cos, "cos", -INFINITY, "-inf");
  check(sin, "sin", INFINITY, "+inf");
  check(sin, "sin", -INFINITY, "-inf");

  puts("trig_inf_domain_error: ok");
  return 0;
}
