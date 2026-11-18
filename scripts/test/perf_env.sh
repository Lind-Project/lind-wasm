#!/usr/bin/env bash
#
# Print a description of this machine and setup for lind-perf result folders (env.txt).
# Used by collect_microbench.sh and collect_appbench.sh.
#
# Usage: scripts/test/perf_env.sh CPU
#   CPU  the CPU the benchmarks are pinned to (its frequency settings are reported)
#
# Every item is optional: a missing file or tool just leaves a gap.

CPU="${1:-0}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cpu_sys="/sys/devices/system/cpu/cpu${CPU}"

section() { printf '\n## %s\n' "$1"; }
show() { # show LABEL FILE...: print the files that exist
  local label="$1"; shift
  local f
  for f in "$@"; do
    [[ -r "${f}" ]] && printf '%-34s %s\n' "${label}" "$(tr '\n' ' ' < "${f}")"
  done
  return 0
}

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
show "cpu${CPU} frequency driver" "${cpu_sys}/cpufreq/scaling_driver"
show "cpu${CPU} governor" "${cpu_sys}/cpufreq/scaling_governor"
show "cpu${CPU} energy/perf preference" "${cpu_sys}/cpufreq/energy_performance_preference"
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
exit 0
