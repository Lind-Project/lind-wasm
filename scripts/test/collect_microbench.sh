#!/usr/bin/env bash
#
# lind-perf: repeat the microbenchmarks on one machine and record the machine.
#
# Runs scripts/test/run_microbench.sh RUNS times pinned to one CPU, saves each run's output,
# records the hardware/kernel/container setup (env.txt), and summarizes every counter across
# the runs (median, min, max) into summary.txt and summary.csv. Copy the whole output directory
# back to compare setups.
#
# Needs build/lind-boot and build/lind-boot-perf (make lind-boot lind-boot-perf).
#
# Usage: scripts/test/collect_microbench.sh [-n RUNS] [-c CPU] [-l LABEL] [-o DIR] [-b N] [-- ARGS]
#   -n RUNS   repetitions (default 5)
#   -c CPU    CPU to pin to with taskset (default 2)
#   -l LABEL  name for this setup, used in the output directory (default: hostname)
#   -o DIR    output directory (default: ~/lind-perf-results/LABEL-YYYYmmdd-HHMMSS)
#   -b N      also run benchrunner.py (native Linux vs Lind, normal build) N times, pinned to the
#             same CPU, into benchrunner-pinned.txt (default 0; tests: $BENCH_TESTS, default
#             "sys_close sys_geteuid")
#   ARGS      passed to run_microbench.sh, e.g. -- tsc  or  -- clock --perf-filter=lind_3i::

set -euo pipefail

RUNS=5
CPU=2
LABEL=""
OUT=""
BENCH_RUNS=0
BENCH_TESTS="${BENCH_TESTS:-sys_close sys_geteuid}"
while getopts "n:c:l:o:b:h" opt; do
  case "${opt}" in
    n) RUNS="${OPTARG}" ;;
    b) BENCH_RUNS="${OPTARG}" ;;
    c) CPU="${OPTARG}" ;;
    l) LABEL="${OPTARG}" ;;
    o) OUT="${OPTARG}" ;;
    h) sed -n '3,24p' "$0"; exit 0 ;;
    *) echo "see: $0 -h" >&2; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
EXTRA_ARGS=("$@")

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
LABEL="${LABEL:-$(hostname)}"
OUT="${OUT:-${HOME}/lind-perf-results/${LABEL}-$(date +%Y%m%d-%H%M%S)}"

for bin in build/lind-boot build/lind-boot-perf; do
  if [[ ! -x "${REPO_ROOT}/${bin}" ]]; then
    echo "error: ${bin} not found; run 'make lind-boot lind-boot-perf'" >&2
    exit 2
  fi
done
if ! taskset -c "${CPU}" true 2>/dev/null; then
  echo "error: cannot pin to CPU ${CPU}; allowed: $(taskset -cp $$ | sed 's/.*: //')" >&2
  exit 2
fi
mkdir -p "${OUT}"

# ---------------------------------------------------------------------------------------------
# Record the setup.
# ---------------------------------------------------------------------------------------------
{
  echo "# lind-perf setup: ${LABEL}"
  echo "date                               $(date -Is)"
  echo "runs                               ${RUNS}"
  echo "pinned cpu                         ${CPU}"
  echo "run_microbench args                ${EXTRA_ARGS[*]:-(none)}"
  echo "benchrunner runs                   ${BENCH_RUNS} (${BENCH_TESTS})"
  "${SCRIPT_DIR}/perf_env.sh" "${CPU}"
} > "${OUT}/env.txt"

# ---------------------------------------------------------------------------------------------
# Runs
# ---------------------------------------------------------------------------------------------
echo "Results: ${OUT}"
for ((i = 1; i <= RUNS; i++)); do
  printf 'run %d/%d ... ' "${i}" "${RUNS}"
  start=$(date +%s)
  if taskset -c "${CPU}" "${SCRIPT_DIR}/run_microbench.sh" ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"} \
      > "${OUT}/run-${i}.txt" 2>&1; then
    echo "done ($(( $(date +%s) - start )) s, load $(cut -d' ' -f1-3 /proc/loadavg))"
  else
    echo "FAILED, see ${OUT}/run-${i}.txt" >&2
    exit 1
  fi
done
if (( BENCH_RUNS > 0 )); then
  printf 'benchrunner x%d (%s) ... ' "${BENCH_RUNS}" "${BENCH_TESTS}"
  {
    git -C "${REPO_ROOT}" rev-parse --abbrev-ref HEAD
    git -C "${REPO_ROOT}" log --oneline -1
    for ((i = 1; i <= BENCH_RUNS; i++)); do
      # shellcheck disable=SC2086
      taskset -c "${CPU}" "${SCRIPT_DIR}/benchrunner.py" ${BENCH_TESTS}
    done
  } > "${OUT}/benchrunner-pinned.txt" 2>&1 && echo done || echo "FAILED, see benchrunner-pinned.txt"
fi
{
  echo
  echo "## Load at end"
  echo "loadavg                            $(cat /proc/loadavg 2>/dev/null)"
  [[ -r /sys/fs/cgroup/cpu.stat ]] && echo "cgroup cpu.stat                    $(tr '\n' ' ' < /sys/fs/cgroup/cpu.stat)"
} >> "${OUT}/env.txt"

python3 "${SCRIPT_DIR}/summarize_microbench.py" --csv "${OUT}/summary.csv" "${OUT}"/run-*.txt \
  > "${OUT}/summary.txt"
cat "${OUT}/summary.txt"
echo
echo "Saved: ${OUT}/{env.txt,run-*.txt,summary.txt,summary.csv}"
(( BENCH_RUNS > 0 )) && echo "       ${OUT}/benchrunner-pinned.txt"
exit 0
