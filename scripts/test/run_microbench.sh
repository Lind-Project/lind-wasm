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

set -euo pipefail

TIMER="${1:-clock}"
case "${TIMER}" in
  clock|tsc) shift || true ;;
  --*) TIMER=clock ;;
  *) echo "usage: $0 [clock|tsc] [--perf-filter=...]" >&2; exit 2 ;;
esac
EXTRA_ARGS=("$@")

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
for prog in libc_syscall fdtables_syscall null_syscall; do
  "${COMPILE}" --output-dir "${OUT_DIR}" "${BENCH_DIR}/${prog}.c" >/dev/null
done
# Grates must be static: lind-boot only creates a grate worker pool for statically linked
# modules that export pass_fptr_to_wt (same flags as the grate test harness).
"${COMPILE}" -s --output-dir "${OUT_DIR}" --compile-grate "${BENCH_DIR}/grate_syscall.c" >/dev/null

run() {
  local title="$1"; shift
  echo
  echo "=================================================================================================="
  echo " ${title}"
  echo "=================================================================================================="
  # Same preloads as scripts/bin/lind_run. sudo is needed for the chroot into lindfs.
  sudo -E "${PERF_BIN}" \
    --preload env=/lib/libc.cwasm --preload env=/lib/libm.cwasm \
    "--perf=${TIMER}" ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"} "$@"
}

run "libc_syscall (2001): kernel call"           "${OUT_DIR}/libc_syscall.cwasm"
run "fdtables_syscall (2002): fdtables, no kernel" "${OUT_DIR}/fdtables_syscall.cwasm"
run "null_syscall (2003): Wasm<->host round trip" "${OUT_DIR}/null_syscall.cwasm"
run "grate + libc_syscall"     "${OUT_DIR}/grate_syscall.cwasm" "${OUT_DIR}/libc_syscall.cwasm"
run "grate + fdtables_syscall" "${OUT_DIR}/grate_syscall.cwasm" "${OUT_DIR}/fdtables_syscall.cwasm"
