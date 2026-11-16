/*
 * lind-perf benchmark: libc_syscall, a RawPOSIX handler that calls the Linux kernel (behaves like geteuid()).
 *
 * Run with scripts/test/run_microbench.sh (needs build/lind-boot-perf).
 */
#include "lind_perf_bench.h"

int main(void) {
  lind_perf_run(libc_syscall);
  return 0;
}
