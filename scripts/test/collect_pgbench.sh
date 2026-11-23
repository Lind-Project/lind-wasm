#!/usr/bin/env bash
#
# lind-perf: PostgreSQL (pgbench) natively, under Lind, and under Lind behind a pass-through
# grate, repeated and pinned.
#
# Each repetition runs every mode in turn (so slow drift affects all modes equally):
#   native  PostgreSQL built for the host from the same source ($PG_NATIVE/bin)
#   lind    PostgreSQL from lind-wasm-apps under Lind (lindfs/bin/*.cwasm, --enable-fpcast)
#   grate   the same, behind testing-grate, which intercepts file, I/O, socket and identity
#           syscalls and forwards them unchanged: the cost of a grate in the path
# For each mode: initdb (not timed), start the server, wait until it accepts connections,
# pgbench -i (not timed), then each workload with each client count, then shutdown (SIGTERM).
# Server, clients and pgbench all run in the same mode: under Lind, inside one lind-boot
# (bash starts them, as in lind-wasm-apps' pg_grate_test.sh).
#
# Workloads (pgbench's built-in scripts; -n, -j 1, -T SECONDS each):
#   select  select-only (pgbench -S): one indexed SELECT per transaction; mostly syscalls + IPC
#   tpcb    TPC-B-like (pgbench default): 3 UPDATEs, a SELECT and an INSERT; writes, WAL, fsync
# Results: tps (without initial connection time) and average latency per workload, client count
# and mode; median, min and max over the repetitions, and Lind/native and grate/Lind ratios.
#
# Needs:
#   - lind-wasm built with fpcast libs (make build WITH_FPCAST=1) and make lind-boot
#   - lind-wasm-apps: make bash postgres; make install-bash install-postgres (LIND_DYLINK=1);
#     lindfs/bin/bash and lindfs/bin/{initdb,postgres,psql,pgbench}.cwasm, executable
#     (chmod +x lindfs/bin/*)
#   - native PostgreSQL from the same source in $PG_NATIVE (default ~/pg-native), built with -O2
#   - for grate mode: lindfs/grates/testing-grate.cwasm
#
# Usage: scripts/test/collect_pgbench.sh [-n RUNS] [-c CPUS] [-l LABEL] [-o DIR] [-m MODES]
#                                        [-w WORKLOADS] [-C CLIENTS] [-T SECONDS] [-s SCALE]
#                                        [-g GRATE] [-p PORT] [-t SECONDS]
#   -n RUNS       repetitions (default 5)
#   -c CPUS       CPUs to pin to with taskset (default 2,3: PostgreSQL is multi-process, and
#                 multi-process programs can fail under Lind when pinned to a single CPU)
#   -l LABEL      name for this setup, used in the output directory (default: hostname)
#   -o DIR        output directory (default: ~/lind-perf-results/LABEL-pgbench-YYYYmmdd-HHMMSS)
#   -m MODES      comma list of native,lind,grate (default: all three)
#   -w WORKLOADS  comma list of select,tpcb (default: both)
#   -C CLIENTS    comma list of pgbench client counts (default 1,4)
#   -T SECONDS    duration of each pgbench run (default 30)
#   -s SCALE      pgbench scale factor (default 10: 1,000,000 accounts, ~150 MB)
#   -g GRATE      pass-through grate inside lindfs (default grates/testing-grate.cwasm)
#   -p PORT       server port (default 54329); the socket is in /tmp (lindfs/tmp under Lind)
#   -t SECONDS    timeout for one mode of one repetition (default 3600)
# Environment: PG_NATIVE (native install prefix, default ~/pg-native).

set -uo pipefail

RUNS=5
CPUS=2,3
LABEL=""
OUT=""
MODES="native,lind,grate"
WORKLOADS="select,tpcb"
CLIENTS="1,4"
DURATION=30
SCALE=10
GRATE="grates/testing-grate.cwasm"
PORT=54329
TIMEOUT=3600
while getopts "n:c:l:o:m:w:C:T:s:g:p:t:h" opt; do
  case "${opt}" in
    n) RUNS="${OPTARG}" ;;
    c) CPUS="${OPTARG}" ;;
    l) LABEL="${OPTARG}" ;;
    o) OUT="${OPTARG}" ;;
    m) MODES="${OPTARG}" ;;
    w) WORKLOADS="${OPTARG}" ;;
    C) CLIENTS="${OPTARG}" ;;
    T) DURATION="${OPTARG}" ;;
    s) SCALE="${OPTARG}" ;;
    g) GRATE="${OPTARG}" ;;
    p) PORT="${OPTARG}" ;;
    t) TIMEOUT="${OPTARG}" ;;
    h) sed -n '3,46p' "$0"; exit 0 ;;
    *) echo "see: $0 -h" >&2; exit 2 ;;
  esac
done

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
LINDFS="${REPO_ROOT}/lindfs"
LIND_RUN="${REPO_ROOT}/scripts/bin/lind_run"
LABEL="${LABEL:-$(hostname)}"
OUT="${OUT:-${HOME}/lind-perf-results/${LABEL}-pgbench-$(date +%Y%m%d-%H%M%S)}"
PG_NATIVE="${PG_NATIVE:-${HOME}/pg-native}"

# Same default list as collect_appbench.sh: file, I/O, socket, directory and identity calls.
DEFAULT_GRATE_SYSCALLS="0,1,2,3,4,5,6,7,8,16,17,18,19,20,21,22,23,32,33,39,41,42,43,44,45,48,49,50,53,54,55,72,74,76,77,78,79,80,81,82,83,84,86,87,88,89,90,91,102,104,107,108,110,137,138,213,232,233,257,262,263,292,293"
GRATE_RULES="$(printf '%s' "${DEFAULT_GRATE_SYSCALLS}" | sed 's/,/:,/g; s/$/:/')"

# Server settings used in every mode (the ones lind-wasm-apps uses for PostgreSQL under Lind).
INITDB_OPTS="-c max_parallel_workers=0 -c max_parallel_workers_per_gather=0 -c io_method=sync"
# Paths of the data directory and socket directory as each mode sees them.
LIND_PGDATA=/tmp/pgbench-data        # inside Lind; on the host: lindfs/tmp/pgbench-data
LIND_SOCK=/tmp
NATIVE_PGDATA=/tmp/pgbench-native-data
NATIVE_SOCK=/tmp/pgbench-native-sock
INNER_LIND=/pgbench_run.sh            # inside Lind; on the host: lindfs/pgbench_run.sh

die() { echo "error: $*" >&2; exit 2; }
want() { [[ ",${MODES}," == *",$1,"* ]]; }
taskset -c "${CPUS}" true 2>/dev/null || die "cannot pin to CPUs ${CPUS}"
if want lind || want grate; then
  [[ -x "${REPO_ROOT}/build/lind-boot" ]] || die "build/lind-boot not found; run 'make lind-boot'"
  [[ -e "${LINDFS}/bin/bash" || -e "${LINDFS}/bin/bash.cwasm" ]] || die "lindfs/bin/bash not found (lind-wasm-apps: make install-bash)"
  for b in initdb postgres psql pgbench; do
    [[ -e "${LINDFS}/bin/${b}.cwasm" ]] || die "lindfs/bin/${b}.cwasm not found (lind-wasm-apps: make install-bash install-postgres)"
  done
fi
if want native; then
  for b in initdb postgres psql pgbench; do
    [[ -x "${PG_NATIVE}/bin/${b}" ]] || die "${PG_NATIVE}/bin/${b} not found (set PG_NATIVE to the native install prefix)"
  done
  [[ $EUID -ne 0 ]] || die "run as a normal user: native PostgreSQL refuses to start as root"
fi
if want grate && [[ ! -e "${LINDFS}/${GRATE}" ]]; then
  echo "note: ${LINDFS}/${GRATE} not found, so grate mode is skipped" >&2
  MODES="${MODES//grate/}"
fi
mkdir -p "${OUT}/raw"

# ---------------------------------------------------------------------------------------------
# The script that runs one mode: start the server, wait, initialize, run the workloads, stop.
# Written once per mode with that mode's binaries and paths. It avoids external commands other
# than the PostgreSQL programs (and sleep, if present), since lindfs may have no coreutils.
# Each pgbench run is framed by "##PGB begin NAME" / "##PGB end NAME rc=N" for the summary.
# ---------------------------------------------------------------------------------------------
write_inner() { # write_inner FILE BIN_PREFIX BIN_SUFFIX PGDATA SOCK
  local file="$1" pre="$2" suf="$3" pgdata="$4" sock="$5" w c flag
  {
    echo '#!/bin/bash'
    echo "PG=\"${pre}postgres${suf}\"; PSQL=\"${pre}psql${suf}\"; PGBENCH=\"${pre}pgbench${suf}\""
    echo "\"\${PG}\" -D ${pgdata} -k ${sock} -p ${PORT} &"
    echo 'pg_pid=$!'
    echo 'ready=0'
    echo 'for ((t = 0; t < 300; t++)); do'
    echo "  if \"\${PSQL}\" -h ${sock} -p ${PORT} -d postgres -Atqc 'SELECT 1' >/dev/null 2>&1; then ready=1; break; fi"
    echo '  sleep 1 2>/dev/null || true'
    echo 'done'
    echo 'if [ "${ready}" != 1 ]; then echo "##PGB error: server did not accept connections"; kill ${pg_pid}; wait ${pg_pid}; exit 3; fi'
    echo 'echo "##PGB server ready"'
    echo "\"\${PGBENCH}\" -h ${sock} -p ${PORT} -i -s ${SCALE} --no-vacuum -q postgres || { echo '##PGB error: pgbench -i failed'; kill \${pg_pid}; wait \${pg_pid}; exit 4; }"
    echo 'echo "##PGB initialized"'
    for w in ${WORKLOADS//,/ }; do
      case "${w}" in select) flag="-S" ;; tpcb) flag="" ;; *) die "unknown workload ${w}" ;; esac
      for c in ${CLIENTS//,/ }; do
        echo "echo '##PGB begin ${w}-c${c}'"
        echo "\"\${PGBENCH}\" -h ${sock} -p ${PORT} -n ${flag} -c ${c} -j 1 -T ${DURATION} postgres"
        echo "echo \"##PGB end ${w}-c${c} rc=\$?\""
      done
    done
    echo 'kill ${pg_pid}'
    echo 'wait ${pg_pid}'
    echo 'echo "##PGB server stopped"'
  } > "${file}"
  chmod +x "${file}"
}

lind_sudo() { "$@" 2>/dev/null || sudo "$@"; }   # lindfs files written by lind-boot belong to root

# Run one mode once; its output goes to raw/MODE.RUN.txt.
run_mode() { # run_mode MODE RUN
  local mode="$1" i="$2" raw="${OUT}/raw/$1.$2.txt" rc
  case "${mode}" in
    native)
      rm -rf "${NATIVE_PGDATA}" "${NATIVE_SOCK}"; mkdir -p "${NATIVE_SOCK}"
      # shellcheck disable=SC2086
      "${PG_NATIVE}/bin/initdb" -D "${NATIVE_PGDATA}" ${INITDB_OPTS} > "${raw}.initdb" 2>&1 \
        || { echo "initdb failed: ${raw}.initdb"; return 5; }
      timeout -k 10 "${TIMEOUT}" taskset -c "${CPUS}" bash "${OUT}/inner-native.sh" > "${raw}" 2>&1
      rc=$?
      ;;
    lind|grate)
      lind_sudo rm -rf "${LINDFS}${LIND_PGDATA}"
      lind_sudo rm -f "${LINDFS}${LIND_SOCK}/.s.PGSQL.${PORT}" "${LINDFS}${LIND_SOCK}/.s.PGSQL.${PORT}.lock"
      # shellcheck disable=SC2086
      "${LIND_RUN}" --enable-fpcast /bin/initdb.cwasm -D "${LIND_PGDATA}" ${INITDB_OPTS} > "${raw}.initdb" 2>&1 \
        || { echo "initdb failed: ${raw}.initdb"; return 5; }
      if [[ "${mode}" == lind ]]; then
        timeout -k 10 "${TIMEOUT}" taskset -c "${CPUS}" "${LIND_RUN}" --enable-fpcast \
          /bin/bash "${INNER_LIND}" > "${raw}" 2>&1
      else
        timeout -k 10 "${TIMEOUT}" taskset -c "${CPUS}" "${LIND_RUN}" --enable-fpcast \
          "${GRATE}" -s "${GRATE_RULES}" /bin/bash "${INNER_LIND}" > "${raw}" 2>&1
      fi
      rc=$?
      ;;
  esac
  return "${rc}"
}

# ---------------------------------------------------------------------------------------------
# Setup record and the per-mode scripts
# ---------------------------------------------------------------------------------------------
{
  echo "# lind-perf pgbench: ${LABEL}"
  echo "date                               $(date -Is)"
  echo "runs                               ${RUNS}"
  echo "pinned cpus                        ${CPUS}"
  echo "modes                              ${MODES}"
  echo "workloads / clients                ${WORKLOADS} / ${CLIENTS} (-j 1, -T ${DURATION} s each, -n)"
  echo "scale                              ${SCALE}"
  echo "server settings (initdb -c)        ${INITDB_OPTS}"
  echo "native PostgreSQL                  ${PG_NATIVE} ($("${PG_NATIVE}/bin/postgres" --version 2>/dev/null))"
  echo "grate                              ${GRATE}"
  echo "grate syscalls                     ${DEFAULT_GRATE_SYSCALLS}"
  for b in bash initdb postgres psql pgbench; do
    printf '%-34s %s\n' "lindfs/bin/${b}.cwasm" "$(ls -l --time-style=+%F' '%T "${LINDFS}/bin/${b}.cwasm" 2>/dev/null | awk '{print $5, $6, $7}')"
  done
  "${SCRIPT_DIR}/perf_env.sh" "${CPUS%%,*}"
} > "${OUT}/env.txt"

if want native; then write_inner "${OUT}/inner-native.sh" "${PG_NATIVE}/bin/" "" "${NATIVE_PGDATA}" "${NATIVE_SOCK}"; fi
if want lind || want grate; then
  write_inner "${OUT}/inner-lind.sh" "/bin/" ".cwasm" "${LIND_PGDATA}" "${LIND_SOCK}"
  cp "${OUT}/inner-lind.sh" "${LINDFS}${INNER_LIND}" 2>/dev/null || sudo cp "${OUT}/inner-lind.sh" "${LINDFS}${INNER_LIND}"
fi

# ---------------------------------------------------------------------------------------------
# Runs
# ---------------------------------------------------------------------------------------------
modes=()
for m in native lind grate; do want "${m}" && modes+=("${m}"); done
(( ${#modes[@]} > 0 )) || die "no modes to run"
RESULTS="${OUT}/results.tsv"
printf 'mode\trun\texit\twall_ns\toutput\n' > "${RESULTS}"
echo "Results: ${OUT}"
echo "Each mode of each run takes about $(( $(echo "${WORKLOADS//,/ }" | wc -w) * $(echo "${CLIENTS//,/ }" | wc -w) * DURATION )) s of pgbench plus setup."
for ((i = 1; i <= RUNS; i++)); do
  for mode in "${modes[@]}"; do
    printf 'run %d/%d %-7s ... ' "${i}" "${RUNS}" "${mode}"
    start=$(date +%s%N)
    run_mode "${mode}" "${i}"
    rc=$?
    end=$(date +%s%N)
    printf '%s\t%d\t%d\t%d\t%s\n' "${mode}" "${i}" "${rc}" "$((end - start))" "raw/${mode}.${i}.txt" >> "${RESULTS}"
    echo "exit ${rc} ($(( (end - start) / 1000000000 )) s)"
  done
done
lind_sudo rm -f "${LINDFS}${INNER_LIND}"

echo "## Load at end" >> "${OUT}/env.txt"
echo "loadavg                            $(cat /proc/loadavg 2>/dev/null)" >> "${OUT}/env.txt"

# ---------------------------------------------------------------------------------------------
# Summary: per workload, client count and mode, median [min .. max] of tps and latency.
# ---------------------------------------------------------------------------------------------
python3 - "${OUT}" <<'PY'
import csv, os, re, statistics, sys
out = sys.argv[1]
rows = []   # per pgbench run
for r in csv.DictReader(open(os.path.join(out, "results.tsv")), delimiter="\t"):
    text = open(os.path.join(out, r["output"]), errors="replace").read()
    for m in re.finditer(r"##PGB begin (\S+)\n(.*?)##PGB end \1 rc=(\d+)", text, re.S):
        name, body, rc = m.group(1), m.group(2), int(m.group(3))
        tps = re.search(r"^tps = ([\d.]+) \(without initial connection time\)", body, re.M)
        lat = re.search(r"^latency average = ([\d.]+) ms", body, re.M)
        txn = re.search(r"^number of transactions actually processed: (\d+)", body, re.M)
        fail = re.search(r"^number of failed transactions: (\d+)", body, re.M)
        rows.append(dict(workload=name, mode=r["mode"], run=r["run"], rc=rc,
                         tps=float(tps.group(1)) if tps else None,
                         latency_ms=float(lat.group(1)) if lat else None,
                         txns=int(txn.group(1)) if txn else None,
                         failed=int(fail.group(1)) if fail else None))
with open(os.path.join(out, "per_run.tsv"), "w") as f:
    w = csv.writer(f, delimiter="\t")
    w.writerow(["workload", "mode", "run", "rc", "tps", "latency_ms", "txns", "failed"])
    for x in rows:
        w.writerow([x[k] for k in ["workload", "mode", "run", "rc", "tps", "latency_ms", "txns", "failed"]])

def stats(vals):
    return (statistics.median(vals), min(vals), max(vals)) if vals else None
workloads = list(dict.fromkeys(x["workload"] for x in rows))
modes = [m for m in ("native", "lind", "grate") if any(x["mode"] == m for x in rows)]
med = {}
lines = ["pgbench summary: median [min .. max] over runs; tps without initial connection time", ""]
csv_rows = [["workload", "mode", "runs_ok", "runs_total", "tps_median", "tps_min", "tps_max",
             "latency_ms_median", "latency_ms_min", "latency_ms_max"]]
for wl in workloads:
    lines.append(wl)
    for m in modes:
        xs = [x for x in rows if x["workload"] == wl and x["mode"] == m]
        ok = [x for x in xs if x["rc"] == 0 and x["tps"] is not None and not x["failed"]]
        t = stats([x["tps"] for x in ok]); l = stats([x["latency_ms"] for x in ok if x["latency_ms"] is not None])
        if t:
            med[(wl, m)] = (t[0], l[0] if l else None)
            lines.append(f"  {m:<7} tps {t[0]:10.1f} [{t[1]:10.1f} .. {t[2]:10.1f}]"
                         + (f"   latency {l[0]:9.3f} ms [{l[1]:.3f} .. {l[2]:.3f}]" if l else "")
                         + ("" if len(ok) == len(xs) else f"   ({len(ok)}/{len(xs)} runs ok)"))
            csv_rows.append([wl, m, len(ok), len(xs), f"{t[0]:.2f}", f"{t[1]:.2f}", f"{t[2]:.2f}"]
                            + ([f"{l[0]:.4f}", f"{l[1]:.4f}", f"{l[2]:.4f}"] if l else ["", "", ""]))
        else:
            lines.append(f"  {m:<7} no successful runs ({len(xs)} attempted; see raw/)")
            csv_rows.append([wl, m, 0, len(xs)] + [""] * 6)
    def ratio(a, b, label):
        if (wl, a) in med and (wl, b) in med and med[(wl, b)][0]:
            ta, tb = med[(wl, a)][0], med[(wl, b)][0]
            la, lb = med[(wl, a)][1], med[(wl, b)][1]
            extra = f", latency +{la - lb:.3f} ms" if la is not None and lb is not None else ""
            lines.append(f"  {label:<20} tps {ta / tb:.3f}x{extra}")
    ratio("lind", "native", "lind / native")
    ratio("grate", "lind", "grate / lind")
    ratio("grate", "native", "grate / native")
    lines.append("")
open(os.path.join(out, "summary.txt"), "w").write("\n".join(lines))
with open(os.path.join(out, "summary.csv"), "w", newline="") as f:
    csv.writer(f).writerows(csv_rows)
print("\n".join(lines))
PY
echo
echo "Saved: ${OUT}/{env.txt,results.tsv,per_run.tsv,raw/,inner-*.sh,summary.txt,summary.csv}"
echo "A mode with a non-zero exit failed or timed out; see raw/MODE.RUN.txt (and .initdb)."
