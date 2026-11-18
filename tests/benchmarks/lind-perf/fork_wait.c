/*
 * lind-perf benchmark: fork() + waitpid() of a child that exits at once (like benchrunner's
 * "Cage fork + waitpid" in tests/benchmarks/cage_stress.c).
 *
 * The guest's own time per fork + wait is reported as guest::per_call. The fork:: and exit::
 * counters break it down: the parent's setup, unwind and rewind, the child's store, linker,
 * instance and memory copy, and the child's exit.
 *
 * Usage: lind-boot-perf --perf --perf-filter=guest::,fork::,exit:: fork_wait.cwasm
 */
#include "lind_perf_bench.h"

#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>

#define FORK_LOOPS 200
#define FORK_WARMUP 5

static void fork_wait_once(void) {
  pid_t pid = fork();
  if (pid < 0) {
    perror("fork");
    exit(1);
  }
  if (pid == 0) {
    _exit(0);
  }
  int status;
  if (waitpid(pid, &status, 0) < 0) {
    perror("waitpid");
    exit(1);
  }
}

int main(void) {
  lind_perf_cageid = (uint64_t)getpid();
  for (int i = 0; i < FORK_WARMUP; i++) {
    fork_wait_once();
  }
  uint64_t start = lind_perf_now_ns();
  for (int i = 0; i < FORK_LOOPS; i++) {
    fork_wait_once();
  }
  uint64_t end = lind_perf_now_ns();
  lind_perf_call(GUEST_REPORT_SYSCALL_NUM, end - start, FORK_LOOPS);
  return 0;
}
