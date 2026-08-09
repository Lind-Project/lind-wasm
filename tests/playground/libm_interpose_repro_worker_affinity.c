/* Originally written to probe a "worker-affinity" hypothesis for why
 * FE_INVALID reads back NOT SET under interposition for acos/cosh/
 * remainder/lgamma (see libm_interpose_repro_acos.c et al): the theory was
 * that feclearexcept/the real call/fetestexcept are three independently
 * dispatched interposed calls, and if they don't land on the same grate
 * worker (each with its own Store/Instance/TLS), the flag one worker set
 * is invisible to a different worker's fetestexcept() read.
 *
 * MUST BE COMPILED WITH -O0 (see "Run" below). At the default -O2, this
 * loop reads back 0/200 even in BASELINE (no interposition at all) --
 * a separate compiler-codegen artifact, not the bug under test. The
 * `volatile`/getpid() barriers below stop clang from constant-folding the
 * literal acos(1.125) argument, but at -O2 something in the loop's
 * codegen still breaks the feclearexcept -> acos -> fetestexcept
 * ordering; -O0 was the only thing that made *baseline* read back
 * 200/200 as expected. The single-call repros elsewhere in this
 * directory (e.g. libm_interpose_repro_acos.c) don't need -O0 -- this
 * quirk is specific to the loop shape.
 *
 * RESULT (at -O0): DISPROVEN. FE_INVALID correctly SET in 0/200
 * iterations under interposition regardless of LIND_GRATE_WORKERS --
 * including LIND_GRATE_WORKERS=1, which forces every single call onto
 * the *same* worker, eliminating any possibility of cross-worker
 * inconsistency. The failure is fully deterministic (100%, not flaky),
 * which rules out a pool-scheduling race entirely. Whatever the real
 * root cause is, it reproduces even within one worker's own self-
 * consistent TLS state across three *separate* dispatched calls -- worth
 * investigating next (e.g. whether the software-simulated fenv state in
 * sysdeps/lind/fenv_libc.h actually survives across distinct
 * dispatch_lib_call invocations the way real TLS should, even without a
 * worker switch), but that's a different question than the one this file
 * was written to test.
 *
 * Kept as a loop instead of the single-call repro (libm_interpose_repro_
 * acos.c already reproduces the underlying bug in one shot) specifically
 * so LIND_GRATE_WORKERS could be swept -- useful for future investigation
 * even though it didn't confirm the original hypothesis.
 *
 * Run (note the -O0, required -- see above):
 *   scripts/lind_compile tests/playground/libm_interpose_repro_worker_affinity.c -lm -O0
 *   cp tests/playground/libm_interpose_repro_worker_affinity.wasm lindfs/
 *   scripts/lind_run libm_interpose_repro_worker_affinity.wasm   # baseline: expect 200/200
 *   LIND_LIBM_MODE=interposed LIND_GRATE_WORKERS=1 scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_worker_affinity.wasm
 *   LIND_LIBM_MODE=interposed LIND_GRATE_WORKERS=8 scripts/lind_run \
 *     grates/libm_full_grate.cwasm libm_interpose_repro_worker_affinity.wasm
 */
#include <fenv.h>
#include <math.h>
#include <stdio.h>
#include <unistd.h>

#define ITERATIONS 200

int main(void) {
  int set_count = 0;

  for (int i = 0; i < ITERATIONS; i++) {
    /* volatile: without this, clang constant-folds acos(1.125) (a literal
     * argument) at compile time -- since it doesn't model the fenv side
     * effect, the *runtime* call (and hence any interposition dispatch)
     * never happens at all, and every iteration spuriously reads back
     * NOT SET even in baseline. */
    volatile double x = 1.125;
    feclearexcept(FE_ALL_EXCEPT);
    double r = acos(x);
    /* getpid(): a harmless intervening call. Without *some* call between
     * the triggering call and fetestexcept(), this unreliably reads back
     * NOT SET on this target even in baseline -- same shape as the
     * "read errno/flag state too early" quirk documented in
     * repro_errno_propagation.c and comp_errno_propagation.md. The
     * single-shot repros elsewhere in this directory have this "for free"
     * because they printf() the acos() result before checking the flag;
     * this tight loop doesn't print per-iteration, so it needs an
     * explicit stand-in. */
    getpid();
    int is_set = fetestexcept(FE_INVALID) != 0;
    (void)r;
    if (is_set) {
      set_count++;
    }
  }

  printf("FE_INVALID correctly SET in %d/%d iterations (expected %d/%d)\n",
         set_count, ITERATIONS, ITERATIONS, ITERATIONS);
  return 0;
}
