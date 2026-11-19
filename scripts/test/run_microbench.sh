#!/usr/bin/env bash
#
# lind-perf: per-layer syscall microbenchmarks.
#
# Needs both binaries:
#   make lind-boot        # build/lind-boot       (lind_compile precompiles with it)
#   make lind-boot-perf   # build/lind-boot-perf  (runs the benchmarks with --perf)
#
# Usage: scripts/test/run_microbench.sh [clock|tsc] [--perf-filter=PREFIX[,PREFIX...]]
#
# Benchmarks (tests/benchmarks/lind-perf):
#   libc       2001, RawPOSIX handler that calls the kernel
#   fdtables   2002, RawPOSIX handler resolved in fdtables
#   null       2003, returns at the host syscall entry: Wasm<->host round trip
#   grate+libc / grate+fdtables: the same calls interposed by a no-op grate
#   forwarding grate+libc / +fdtables: interposed by a grate that forwards them to RawPOSIX,
#     like a real grate does; compare with libc / fdtables for the cost of a grate in the path
#   fork_wait  fork() + waitpid() of a child that exits at once, broken down by the fork:: and
#     exit:: counters
#
# Each benchmark runs only the counters on its path, unless --perf-filter is given.

set -euo pipefail

TIMER="${1:-clock}"
case "${TIMER}" in
  clock|tsc) shift || true ;;
  --*) TIMER=clock ;;
  *) echo "usage: $0 [clock|tsc] [--perf-filter=...]" >&2; exit 2 ;;
esac
EXTRA_ARGS=("$@")
USER_FILTER=0
for arg in "$@"; do
  [[ "${arg}" == --perf-filter* ]] && USER_FILTER=1
done
SYSCALL_COUNTERS="guest::,lind_common::,threei::,rawposix::libc_call,rawposix::fdtables_call,fdtables::,lind_boot::,lind_3i::,span::"
FORK_COUNTERS="guest::,fork::,exit::"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
BENCH_DIR="${REPO_ROOT}/tests/benchmarks/lind-perf"
COMPILE="${REPO_ROOT}/scripts/bin/lind_compile"
PERF_BIN="${REPO_ROOT}/build/lind-boot-perf"
OUT_DIR="lind-perf" # inside lindfs

if [[ ! -x "${PERF_BIN}" ]]; then
  echo "error: ${PERF_BIN} not found; run 'make lind-boot-perf'" >&2
  exit 2
fi

echo "Compiling benchmarks into lindfs/${OUT_DIR}/ ..."
for prog in libc_syscall fdtables_syscall null_syscall fork_wait; do
  "${COMPILE}" --output-dir "${OUT_DIR}" "${BENCH_DIR}/${prog}.c" >/dev/null
done
# Grates must be static: lind-boot only creates a grate worker pool for statically linked
# modules that export pass_fptr_to_wt (same flags as the grate test harness).
for grate in grate_syscall grate_forward; do
  "${COMPILE}" -s --output-dir "${OUT_DIR}" --compile-grate "${BENCH_DIR}/${grate}.c" >/dev/null
done

# run TITLE COUNTER_PREFIXES PROGRAM... (the prefixes are ignored if --perf-filter was given)
run() {
  local title="$1"; shift
  local filter=("--perf-filter=$1"); shift
  [[ "${USER_FILTER}" == 1 ]] && filter=()
  echo
  echo "=================================================================================================="
  echo " ${title}"
  echo "=================================================================================================="
  # Same preloads as scripts/bin/lind_run. sudo is needed for the chroot into lindfs.
  sudo -E "${PERF_BIN}" \
    --preload env=/lib/libc.cwasm --preload env=/lib/libm.cwasm \
    "--perf=${TIMER}" ${filter[@]+"${filter[@]}"} ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"} "$@"
}

run "libc_syscall (2001): kernel call" "${SYSCALL_COUNTERS}" "${OUT_DIR}/libc_syscall.cwasm"
run "fdtables_syscall (2002): fdtables, no kernel" "${SYSCALL_COUNTERS}" \
  "${OUT_DIR}/fdtables_syscall.cwasm"
run "null_syscall (2003): Wasm<->host round trip" "${SYSCALL_COUNTERS}" \
  "${OUT_DIR}/null_syscall.cwasm"
run "grate + libc_syscall" "${SYSCALL_COUNTERS}" \
  "${OUT_DIR}/grate_syscall.cwasm" "${OUT_DIR}/libc_syscall.cwasm"
run "grate + fdtables_syscall" "${SYSCALL_COUNTERS}" \
  "${OUT_DIR}/grate_syscall.cwasm" "${OUT_DIR}/fdtables_syscall.cwasm"
run "forwarding grate + libc_syscall: cage -> grate -> RawPOSIX -> kernel" "${SYSCALL_COUNTERS}" \
  "${OUT_DIR}/grate_forward.cwasm" "${OUT_DIR}/libc_syscall.cwasm"
run "forwarding grate + fdtables_syscall: cage -> grate -> RawPOSIX -> fdtables" "${SYSCALL_COUNTERS}" \
  "${OUT_DIR}/grate_forward.cwasm" "${OUT_DIR}/fdtables_syscall.cwasm"
run "fork_wait: fork() + waitpid()" "${FORK_COUNTERS}" "${OUT_DIR}/fork_wait.cwasm"
