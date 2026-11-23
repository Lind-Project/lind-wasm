/*
 * Guest side of the lind-perf benchmarks.
 *
 * These syscall numbers exist only in perf builds of lind-boot (`make lind-boot-perf`).
 * The wrappers are defined here rather than in glibc, so normal builds of libc are unchanged.
 *
 *   2001 LIBC_SYSCALL      RawPOSIX handler that calls the kernel (like geteuid()).
 *   2002 FDTABLES_SYSCALL  RawPOSIX handler resolved inside fdtables (like close(-1)).
 *   2003 NULL_SYSCALL      Returns at the host syscall entry, before 3i: measures the
 *                          pure Wasm -> host -> Wasm round trip.
 *   2004 GUEST_REPORT      Hands the guest's own loop timing to lind-perf.
 */
#ifndef LIND_PERF_BENCH_H
#define LIND_PERF_BENCH_H

#include <lind_syscall.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>

#define LIBC_SYSCALL_NUM 2001
#define FDTABLES_SYSCALL_NUM 2002
#define NULL_SYSCALL_NUM 2003
#define GUEST_REPORT_SYSCALL_NUM 2004

#define LIND_PERF_UNUSED 0xdeadbeefdeadbeefULL
#define LIND_PERF_LOOPS 1000000
#define LIND_PERF_WARMUP 1000

/* In Lind, getpid() returns the cage id. Looked up once, outside the timed loop. */
static uint64_t lind_perf_cageid;

static inline int64_t lind_perf_call(unsigned int num, uint64_t arg1, uint64_t arg2) {
  uint64_t self = lind_perf_cageid;
  return make_threei_call(num, LIND_PERF_UNUSED, self, self,
                          arg1, self, arg2, self,
                          LIND_PERF_UNUSED, self, LIND_PERF_UNUSED, self,
                          LIND_PERF_UNUSED, self, LIND_PERF_UNUSED, self,
                          0 /* raw result, no errno translation */);
}

static inline int64_t libc_syscall(void) {
  return lind_perf_call(LIBC_SYSCALL_NUM, LIND_PERF_UNUSED, LIND_PERF_UNUSED);
}

/* fd is always -1, so fdtables rejects it without entering the kernel. */
static inline int64_t fdtables_syscall(void) {
  return lind_perf_call(FDTABLES_SYSCALL_NUM, (uint64_t)-1, LIND_PERF_UNUSED);
}

static inline int64_t null_syscall(void) {
  return lind_perf_call(NULL_SYSCALL_NUM, LIND_PERF_UNUSED, LIND_PERF_UNUSED);
}

static inline uint64_t lind_perf_now_ns(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/*
 * Run `call` LIND_PERF_LOOPS times (after a short warm-up), time the loop from inside the
 * guest, and report it. The report shows up as guest::per_call.
 */
static inline void lind_perf_run(int64_t (*call)(void)) {
  lind_perf_cageid = (uint64_t)getpid();
  for (int i = 0; i < LIND_PERF_WARMUP; i++) {
    call();
  }
  uint64_t start = lind_perf_now_ns();
  for (int i = 0; i < LIND_PERF_LOOPS; i++) {
    call();
  }
  uint64_t end = lind_perf_now_ns();
  lind_perf_call(GUEST_REPORT_SYSCALL_NUM, end - start, LIND_PERF_LOOPS);
}

#endif /* LIND_PERF_BENCH_H */
