#!/usr/bin/env bash
#
# lind-perf: real programs (lmbench and whole applications) natively, under Lind, and under Lind
# behind a pass-through grate, repeated and pinned to one CPU.
#
# For every workload in the workload file (default scripts/test/appbench_workloads.txt) and every
# repetition, runs each mode in turn:
#   native  the program built for the host
#   lind    the same program under Lind (scripts/bin/lind_run, normal lind-boot build)
#   grate   the same, behind testing-grate (lind-wasm-example-grates), which intercepts the
#           workload's syscalls and forwards them unchanged: the cost of a grate in the path
# and saves every output, env.txt (the machine setup) and summary.txt / summary.csv (median,
# min and max per workload and mode, and the differences between modes).
#
# Needs: build/lind-boot (make lind-boot); lind-wasm-apps built and installed into lindfs
# (lmbench and, for whole-program workloads, bash / cpython / grep); a native lmbench
# ($LMBENCH_NATIVE); for grate mode, grates/testing-grate.cwasm in lindfs.
#
# Usage: scripts/test/collect_appbench.sh [-n RUNS] [-c CPU] [-l LABEL] [-o DIR] [-w FILE]
#                                         [-m MODES] [-f REGEX] [-g GRATE] [-t SECONDS]
#   -n RUNS     repetitions (default 5)
#   -c CPU      CPU to pin to with taskset (default 2)
#   -l LABEL    name for this setup, used in the output directory (default: hostname)
#   -o DIR      output directory (default: ~/lind-perf-results/LABEL-apps-YYYYmmdd-HHMMSS)
#   -w FILE     workload file (default: scripts/test/appbench_workloads.txt)
#   -m MODES    comma list of native,lind,grate (default: all three)
#   -f REGEX    only workloads whose name matches (e.g. '^lat_' or 'python')
#   -g GRATE    pass-through grate inside lindfs (default: grates/testing-grate.cwasm)
#   -t SECONDS  timeout per run (default 600)
# Environment: LMBENCH_NATIVE (default ~/lmbench-native/bin/native), LMBENCH_LIND (lmbench dir
# inside lindfs, default bin), APPS_LIND (app dir inside lindfs, default usr/local/bin).

set -uo pipefail

RUNS=5
CPU=2
LABEL=""
OUT=""
WORKLOADS=""
MODES="native,lind,grate"
FILTER=""
GRATE="grates/testing-grate.cwasm"
TIMEOUT=600
while getopts "n:c:l:o:w:m:f:g:t:h" opt; do
  case "${opt}" in
    n) RUNS="${OPTARG}" ;;
    c) CPU="${OPTARG}" ;;
    l) LABEL="${OPTARG}" ;;
    o) OUT="${OPTARG}" ;;
    w) WORKLOADS="${OPTARG}" ;;
    m) MODES="${OPTARG}" ;;
    f) FILTER="${OPTARG}" ;;
    g) GRATE="${OPTARG}" ;;
    t) TIMEOUT="${OPTARG}" ;;
    h) sed -n '3,33p' "$0"; exit 0 ;;
    *) echo "see: $0 -h" >&2; exit 2 ;;
  esac
done

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
LINDFS="${REPO_ROOT}/lindfs"
LIND_RUN="${REPO_ROOT}/scripts/bin/lind_run"
WORKLOADS="${WORKLOADS:-${SCRIPT_DIR}/appbench_workloads.txt}"
LABEL="${LABEL:-$(hostname)}"
OUT="${OUT:-${HOME}/lind-perf-results/${LABEL}-apps-$(date +%Y%m%d-%H%M%S)}"
LMN="${LMBENCH_NATIVE:-${HOME}/lmbench-native/bin/native}"
LML="${LMBENCH_LIND:-bin}"
APPS="${APPS_LIND:-usr/local/bin}"
DATA="/tmp/lindperf-apps"

# Syscalls the pass-through grate intercepts by default: file, I/O, socket, directory and identity
# calls. Process control (clone/fork/exec/exit/wait/kill), signals, memory (mmap/brk/mprotect),
# futex and sleep are left alone. Linux x86-64 numbers, as used by Lind.
DEFAULT_GRATE_SYSCALLS="0,1,2,3,4,5,6,7,8,16,17,18,19,20,21,22,23,32,33,39,41,42,43,44,45,48,49,50,53,54,55,72,74,76,77,78,79,80,81,82,83,84,86,87,88,89,90,91,102,104,107,108,110,137,138,213,232,233,257,262,263,292,293"

die() { echo "error: $*" >&2; exit 2; }
[[ -x "${REPO_ROOT}/build/lind-boot" ]] || die "build/lind-boot not found; run 'make lind-boot'"
[[ -f "${WORKLOADS}" ]] || die "workload file ${WORKLOADS} not found"
taskset -c "${CPU}" true 2>/dev/null || die "cannot pin to CPU ${CPU}"
want() { [[ ",${MODES}," == *",$1,"* ]]; }
if want grate && [[ ! -e "${LINDFS}/${GRATE}" ]]; then
  echo "note: ${LINDFS}/${GRATE} not found, so grate mode is skipped. Build it with:" >&2
  echo "  git clone https://github.com/Lind-Project/lind-wasm-example-grates ~/lind-wasm-example-grates" >&2
  echo "  cd ~/lind-wasm-example-grates/rust-grates/testing-grate && cargo lind_compile --release --output-dir grates" >&2
  MODES="${MODES//grate/}"
fi
mkdir -p "${OUT}/raw"

# ---------------------------------------------------------------------------------------------
# Test data, created identically on the host and inside lindfs (same path in both).
# ---------------------------------------------------------------------------------------------
make_data() { # make_data ROOT: ROOT is "" for the host or $LINDFS
  local d="$1${DATA}"
  mkdir -p "${d}/tree" "${d}/py-out" || return 1
  [[ -s "${d}/sample.bin" ]] || dd if=/dev/urandom of="${d}/sample.bin" bs=1M count=8 status=none
  [[ -s "${d}/sample64.bin" ]] || dd if=/dev/urandom of="${d}/sample64.bin" bs=1M count=64 status=none
  python3 - "${d}" <<'PY'
import os, sys
d = sys.argv[1]
tree = os.path.join(d, "tree")
for i in range(20):
    sub = os.path.join(tree, f"dir{i:02d}")
    os.makedirs(sub, exist_ok=True)
    for j in range(25):
        p = os.path.join(sub, f"file{j:02d}.txt")
        if not os.path.exists(p):
            with open(p, "w") as f:
                for k in range(100):
                    f.write(f"line {k} of {i}/{j}: " + ("lind " if (i + j + k) % 7 == 0 else "") + "x" * 40 + "\n")
with open(os.path.join(d, "fileio.py"), "w") as f:
    f.write('''import os, sys
out = sys.argv[1]
data = b"x" * 4096
for i in range(200):
    with open(os.path.join(out, f"f{i}"), "wb") as fh:
        fh.write(data)
total = 0
for i in range(200):
    with open(os.path.join(out, f"f{i}"), "rb") as fh:
        total += len(fh.read())
for i in range(200):
    os.remove(os.path.join(out, f"f{i}"))
''')
PY
}
echo "Preparing test data in ${DATA} (host) and lindfs${DATA} ..."
make_data "" || die "cannot create ${DATA}"
make_data "${LINDFS}" || { sudo mkdir -p "${LINDFS}${DATA}" && sudo chown -R "$(id -u):$(id -g)" "${LINDFS}${DATA}" && make_data "${LINDFS}"; } || die "cannot create lindfs${DATA}"
# lat_proc exec runs /tmp/hello (or /tmp/hello-s for static builds); bash_spawn runs {DATA}/hello.
if [[ -x "${LMN}/hello" ]]; then
  cp "${LMN}/hello" /tmp/hello && cp "${LMN}/hello" /tmp/hello-s && cp "${LMN}/hello" "${DATA}/hello-native"
fi
if [[ -e "${LINDFS}/${LML}/hello" ]]; then
  for dst in "${LINDFS}/tmp/hello" "${LINDFS}/tmp/hello-s" "${LINDFS}${DATA}/hello"; do
    cp "${LINDFS}/${LML}/hello" "${dst}" 2>/dev/null || sudo cp "${LINDFS}/${LML}/hello" "${dst}"
  done
fi
for dev in null zero; do
  [[ -e "${LINDFS}/dev/${dev}" ]] || echo "note: lindfs/dev/${dev} is missing; lat_syscall read/write may fail" >&2
done

# ---------------------------------------------------------------------------------------------
# Setup record
# ---------------------------------------------------------------------------------------------
{
  echo "# lind-perf app benchmarks: ${LABEL}"
  echo "date                               $(date -Is)"
  echo "runs                               ${RUNS}"
  echo "pinned cpu                         ${CPU}"
  echo "modes                              ${MODES}"
  echo "workloads                          ${WORKLOADS}  ${FILTER:+(filter: ${FILTER})}"
  echo "native lmbench                     ${LMN}"
  echo "lind lmbench / apps (in lindfs)    /${LML}  /${APPS}"
  echo "grate                              ${GRATE}"
  echo "default grate syscalls             ${DEFAULT_GRATE_SYSCALLS}"
  "${SCRIPT_DIR}/perf_env.sh" "${CPU}"
} > "${OUT}/env.txt"
cp "${WORKLOADS}" "${OUT}/workloads.txt"

# ---------------------------------------------------------------------------------------------
# Runs
# ---------------------------------------------------------------------------------------------
fill() { # substitute placeholders
  local s="$1"
  s="${s//\{LMN\}/${LMN}}"; s="${s//\{LML\}/${LML}}"; s="${s//\{APPS\}/${APPS}}"; s="${s//\{DATA\}/${DATA}}"
  printf '%s' "${s}"
}
trim() { local s="$1"; s="${s#"${s%%[![:space:]]*}"}"; printf '%s' "${s%"${s##*[![:space:]]}"}"; }
native_exists() { [[ "$1" == */* ]] && [[ -x "$1" ]] || command -v "$1" >/dev/null 2>&1; }
lind_exists() { [[ -e "${LINDFS}/${1#/}" ]]; }

RESULTS="${OUT}/results.tsv"
printf 'workload\tmetric\tmode\trun\texit\twall_ns\toutput\n' > "${RESULTS}"
echo "Results: ${OUT}"

while IFS= read -r line; do
  [[ -z "$(trim "${line}")" || "$(trim "${line}")" == \#* ]] && continue
  IFS='|' read -r name metric ncmd lcmd gsys <<< "${line}"
  name="$(trim "${name}")"; metric="$(trim "${metric}")"
  ncmd="$(fill "$(trim "${ncmd}")")"; lcmd="$(fill "$(trim "${lcmd}")")"; gsys="$(trim "${gsys:-}")"
  [[ -n "${FILTER}" && ! "${name}" =~ ${FILTER} ]] && continue
  eval "native_argv=(${ncmd})"
  eval "lind_argv=(${lcmd})"
  modes=()
  if want native; then
    native_exists "${native_argv[0]}" && modes+=(native) || echo "skip ${name} native: ${native_argv[0]} not found"
  fi
  if lind_exists "${lind_argv[0]}"; then
    want lind && modes+=(lind)
    if want grate && [[ "${gsys}" != "-" ]]; then modes+=(grate); fi
  elif want lind || want grate; then
    echo "skip ${name} lind/grate: lindfs/${lind_argv[0]#/} not found"
  fi
  (( ${#modes[@]} == 0 )) && continue
  rules="$(printf '%s' "${gsys:-${DEFAULT_GRATE_SYSCALLS}}" | sed 's/,/:,/g; s/$/:/')"

  printf '%-20s' "${name}"
  for ((i = 1; i <= RUNS; i++)); do
    for mode in "${modes[@]}"; do
      raw="raw/${name}.${mode}.${i}.txt"
      start=$(date +%s%N)
      case "${mode}" in
        native) (cd "${DATA}" && timeout -k 10 "${TIMEOUT}" taskset -c "${CPU}" "${native_argv[@]}") > "${OUT}/${raw}" 2>&1 ;;
        lind) timeout -k 10 "${TIMEOUT}" taskset -c "${CPU}" "${LIND_RUN}" "${lind_argv[@]}" > "${OUT}/${raw}" 2>&1 ;;
        grate) timeout -k 10 "${TIMEOUT}" taskset -c "${CPU}" "${LIND_RUN}" "${GRATE}" -s "${rules}" "${lind_argv[@]}" > "${OUT}/${raw}" 2>&1 ;;
      esac
      rc=$?
      end=$(date +%s%N)
      printf '%s\t%s\t%s\t%d\t%d\t%d\t%s\n' "${name}" "${metric}" "${mode}" "${i}" "${rc}" "$((end - start))" "${raw}" >> "${RESULTS}"
      [[ ${rc} == 0 ]] && printf '.' || printf 'x'
    done
  done
  echo
done < "${WORKLOADS}"

echo "## Load at end" >> "${OUT}/env.txt"
echo "loadavg                            $(cat /proc/loadavg 2>/dev/null)" >> "${OUT}/env.txt"
python3 "${SCRIPT_DIR}/summarize_appbench.py" --csv "${OUT}/summary.csv" "${OUT}" > "${OUT}/summary.txt"
cat "${OUT}/summary.txt"
echo
echo "Saved: ${OUT}/{env.txt,workloads.txt,results.tsv,raw/,summary.txt,summary.csv}"
echo "x = a run that failed or timed out; see its raw/ output."
