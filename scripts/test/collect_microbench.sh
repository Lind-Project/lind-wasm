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
# Usage: scripts/test/collect_microbench.sh [-n RUNS] [-c CPU] [-l LABEL] [-o DIR] [-- ARGS]
#   -n RUNS   repetitions (default 5)
#   -c CPU    CPU to pin to with taskset (default 2)
#   -l LABEL  name for this setup, used in the output directory (default: hostname)
#   -o DIR    output directory (default: ~/lind-perf-results/LABEL-YYYYmmdd-HHMMSS)
#   ARGS      passed to run_microbench.sh, e.g. -- tsc  or  -- clock --perf-filter=lind_3i::

set -euo pipefail

RUNS=5
CPU=2
LABEL=""
OUT=""
while getopts "n:c:l:o:h" opt; do
  case "${opt}" in
    n) RUNS="${OPTARG}" ;;
    c) CPU="${OPTARG}" ;;
    l) LABEL="${OPTARG}" ;;
    o) OUT="${OPTARG}" ;;
    h) sed -n '3,20p' "$0"; exit 0 ;;
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
# Record the setup. Every command is optional: missing files or tools just leave a gap.
# ---------------------------------------------------------------------------------------------
section() { printf '\n## %s\n' "$1"; }
show() { # show LABEL FILE...: print files that exist
  local label="$1"; shift
  local f
  for f in "$@"; do
    [[ -r "${f}" ]] && printf '%-34s %s\n' "${label}" "$(tr '\n' ' ' < "${f}")"
  done
  return 0
}
cpu_sys="/sys/devices/system/cpu/cpu${CPU}"
{
  echo "# lind-perf setup: ${LABEL}"
  echo "date                               $(date -Is)"
  echo "runs                               ${RUNS}"
  echo "pinned cpu                         ${CPU}"
  echo "run_microbench args                ${EXTRA_ARGS[*]:-(none)}"

  section "lind-wasm"
  echo "commit                             $(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null)"
  echo "branch                             $(git -C "${REPO_ROOT}" rev-parse --abbrev-ref HEAD 2>/dev/null)"
  echo "uncommitted changes                $(git -C "${REPO_ROOT}" status --short 2>/dev/null | wc -l)"
  git -C "${REPO_ROOT}" log --oneline -5 2>/dev/null | sed 's/^/  /'

  section "CPU"
  grep -m1 'model name' /proc/cpuinfo 2>/dev/null | sed 's/^model name\s*:\s*/model                              /'
  echo "online cpus                        $(nproc --all 2>/dev/null)"
  echo "hypervisor flag (VM if yes)        $(grep -q -m1 ' hypervisor' /proc/cpuinfo && echo yes || echo no)"
  command -v systemd-detect-virt >/dev/null && echo "systemd-detect-virt                $(systemd-detect-virt 2>/dev/null || true)"
  show "smt active" /sys/devices/system/cpu/smt/active
  show "cpu${CPU} thread siblings" "${cpu_sys}/topology/thread_siblings_list"
  show "cpu${CPU} governor" "${cpu_sys}/cpufreq/scaling_governor"
  show "cpu${CPU} cur freq (kHz)" "${cpu_sys}/cpufreq/scaling_cur_freq"
  show "cpu${CPU} min/max freq (kHz)" "${cpu_sys}/cpufreq/scaling_min_freq" "${cpu_sys}/cpufreq/scaling_max_freq"
  show "intel_pstate no_turbo" /sys/devices/system/cpu/intel_pstate/no_turbo
  show "cpufreq boost" /sys/devices/system/cpu/cpufreq/boost
  show "clocksource" /sys/devices/system/clocksource/clocksource0/current_clocksource
  command -v lscpu >/dev/null && { echo; lscpu 2>/dev/null | sed 's/^/  /'; }

  section "Kernel"
  echo "uname                              $(uname -a)"
  show "cmdline" /proc/cmdline
  show "isolated cpus" /sys/devices/system/cpu/isolated
  echo "mitigations:"
  for f in /sys/devices/system/cpu/vulnerabilities/*; do
    [[ -r "${f}" ]] && printf '  %-30s %s\n' "$(basename "${f}")" "$(cat "${f}")"
  done

  section "Container / process limits"
  echo "in docker (/.dockerenv)            $([[ -e /.dockerenv ]] && echo yes || echo no)"
  grep -E '^(Seccomp|Seccomp_filters|NoNewPrivs|Cpus_allowed_list):' /proc/self/status 2>/dev/null \
    | awk '{printf "%-34s %s\n", $1, $2}'
  show "cgroup cpuset" /sys/fs/cgroup/cpuset.cpus.effective
  show "cgroup cpu.max (quota period)" /sys/fs/cgroup/cpu.max
  show "cgroup cpu.stat" /sys/fs/cgroup/cpu.stat
  show "cgroup v1 cfs quota/period" /sys/fs/cgroup/cpu/cpu.cfs_quota_us /sys/fs/cgroup/cpu/cpu.cfs_period_us

  section "Load at start"
  show "loadavg" /proc/loadavg
  grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null
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
{ echo; echo "## Load at end"; show "loadavg" /proc/loadavg; show "cgroup cpu.stat" /sys/fs/cgroup/cpu.stat; } >> "${OUT}/env.txt"

python3 "${SCRIPT_DIR}/summarize_microbench.py" --csv "${OUT}/summary.csv" "${OUT}"/run-*.txt \
  > "${OUT}/summary.txt"
cat "${OUT}/summary.txt"
echo
echo "Saved: ${OUT}/{env.txt,run-*.txt,summary.txt,summary.csv}"
