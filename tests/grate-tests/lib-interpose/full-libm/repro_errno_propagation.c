/* Focused reproducer for the errno cage-propagation gap: when a libm call is
 * genuinely interposed (dispatched into a grate's own address space), the
 * real function sets errno inside the GRATE's own TLS slot, which is
 * invisible to the caller's plain `errno` read afterward (errno is accessed
 * via an inlined macro, not a function call, so it never crosses the
 * interposition boundary the way a call to e.g. fetestexcept() does).
 *
 * Calls cos through a function-pointer parameter (matching
 * trig_inf_domain_error.c's proven-reliable style) and reads `errno`
 * directly as a printf argument rather than storing it to a local variable
 * first -- bisected empirically: storing straight into a local
 * (`int got_errno = errno;`) right after the call unreliably reads back 0
 * even in baseline/non-interposed runs, while reading `errno` directly as
 * a later statement's argument (after another call has intervened) is
 * reliable. Root cause not chased further (pre-existing codegen/TLS-access
 * quirk, orthogonal to the cross-cage propagation bug this file targets --
 * see also repro_fenv_categories.c's similar, differently-shaped gotchas).
 *
 * Run without interposition (baseline, real errno.h behavior):
 *   scripts/lind_compile repro_errno_propagation.c -lm
 *   cp repro_errno_propagation.wasm lindfs/ && scripts/lind_run repro_errno_propagation.wasm
 *
 * Run under forced interposition (exercises the propagation fix):
 *   cp repro_errno_propagation.wasm lindfs/
 *   LIND_LIBM_MODE=interposed scripts/lind_run grates/libm_full_grate.cwasm repro_errno_propagation.wasm
 */
#include <errno.h>
#include <math.h>
#include <stdio.h>

static void check(double (*fn)(double), const char *name, double x,
                   const char *label) {
  errno = 0;
  double r = fn(x);
  printf("%s(%s) = %g\n", name, label, r);
  printf("  errno = %d (expected %d / EDOM) %s\n", errno, EDOM,
         errno == EDOM ? "PASS" : "FAIL");
}

int main(void) {
  check(cos, "cos", INFINITY, "+inf");
  return 0;
}
