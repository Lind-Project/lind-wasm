/*
 * lind-perf benchmark: null_syscall, nothing: the call returns at the host syscall entry, before 3i, so the guest timing is the pure Wasm -> host -> Wasm round trip.
 *
 * Run with scripts/test/run_microbench.sh (needs build/lind-boot-perf).
 */
#include "lind_perf_bench.h"

int main(void) {
  lind_perf_run(null_syscall);
  return 0;
}
