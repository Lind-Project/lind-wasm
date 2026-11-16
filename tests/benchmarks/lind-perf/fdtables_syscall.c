/*
 * lind-perf benchmark: fdtables_syscall, a RawPOSIX handler resolved inside fdtables, no kernel call (behaves like close(-1)).
 *
 * Run with scripts/test/run_microbench.sh (needs build/lind-boot-perf).
 */
#include "lind_perf_bench.h"

int main(void) {
  lind_perf_run(fdtables_syscall);
  return 0;
}
