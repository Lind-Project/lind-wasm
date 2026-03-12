#!/usr/bin/env bash
set +e
# Continues past all failures. No -e, -u, or pipefail.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
LIND_WASM_ROOT="${LIND_WASM_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
LIND_APPS_ROOT="${LIND_APPS_ROOT:-$LIND_WASM_ROOT/../lind-wasm-apps}"

NATIVE_BIN="$LIND_WASM_ROOT/build/benchmark/native"
LIND_BIN="$LIND_APPS_ROOT/build/bin/lmbench/wasm32-wasi"
LIND_RUN="$LIND_WASM_ROOT/scripts/lind_run"
LINDFS="$LIND_WASM_ROOT/lindfs"

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
RESULTS_DIR="$LIND_WASM_ROOT/results"
OUTFILE="$RESULTS_DIR/benchmark_${TIMESTAMP}.md"
RAWDIR="$RESULTS_DIR/raw_${TIMESTAMP}"

REPS=11
WARMUP=1
TIMEOUT_SEC=120
RUNS=5

BW_SIZES="4096 8192 16384 32768 65536 131072 262144 524288 1048576 2097152 4194304 8388608 16777216 33554432 67108864"

while [ $# -gt 0 ]; do
  case "$1" in
    --native-only) RUN_LIND=false ;;
    --lind-only)   RUN_NATIVE=false ;;
    --runs)        shift; RUNS="$1" ;;
    *) echo "Usage: sudo $0 [--native-only|--lind-only] [--runs N]"; exit 0 ;;
  esac
  shift
done

RUN_NATIVE="${RUN_NATIVE:-true}"
RUN_LIND="${RUN_LIND:-true}"

mkdir -p "$RESULTS_DIR" "$RAWDIR"

log() { echo "[bench] $*" >&2; }

human_size() {
  local s=$1
  if [ "$s" -ge 1048576 ]; then echo "$((s/1048576))MB"
  elif [ "$s" -ge 1024 ]; then echo "$((s/1024))KB"
  else echo "${s}B"; fi
}

# Extract numeric value from lmbench output
# lat_syscall: "Simple xxx: 0.1234 microseconds" -> 0.1234
# lat_pipe: "Pipe latency: 5.30 microseconds" -> 5.30
# lat_proc: "Process fork+exit: 411.6 microseconds" -> 411.6
# bw_*: "0.065536 4687.94" or "Pipe bandwidth: 2178 MB/sec" -> last number before MB or second column
parse_val() {
  local txt="$1"
  # Try "X.XXXX microseconds" pattern
  local val
  val=$(echo "$txt" | awk '/microseconds/ { for(i=1;i<=NF;i++) if($(i+1)=="microseconds") print $i }' | tail -1)
  if [ -n "$val" ]; then echo "$val"; return; fi
  # Try "X.XX MB/sec" pattern
  val=$(echo "$txt" | awk '/MB\/sec/ { for(i=1;i<=NF;i++) if($(i+1)=="MB/sec") print $i }' | tail -1)
  if [ -n "$val" ]; then echo "$val"; return; fi
  # Try two-column "size value" (bw_mem output)
  val=$(echo "$txt" | awk '/^[0-9]/ { v=$2 } END { if(v) print v }')
  if [ -n "$val" ]; then echo "$val"; return; fi
  echo "N/A"
}

# Run a command N times, compute mean
# Usage: run_multi TAG RUNS cmd [args...]
# Outputs: "mean total_runs" to stdout, saves all raw data
run_multi() {
  local tag="$1"; shift
  local nruns="$1"; shift
  local sum=0
  local count=0
  local i=1
  while [ "$i" -le "$nruns" ]; do
    local out
    out=$("$@" 2>&1) || true
    echo "$out" >> "$RAWDIR/${tag}_run${i}.txt"
    local val
    val=$(parse_val "$out")
    if [ "$val" != "N/A" ] && [ -n "$val" ]; then
      sum=$(awk "BEGIN { printf \"%.6f\", $sum + $val }")
      count=$((count + 1))
    fi
    i=$((i + 1))
  done
  if [ "$count" -gt 0 ]; then
    local mean
    mean=$(awk "BEGIN { printf \"%.4f\", $sum / $count }")
    echo "$mean $count"
  else
    echo "N/A 0"
  fi
}

run_native_multi() {
  local tag="$1" bin="$2"; shift 2
  log "  [native] $bin $* (x$RUNS)"
  run_multi "${tag}_native" "$RUNS" "$NATIVE_BIN/$bin" "$@"
}

run_lind_multi() {
  local tag="$1" bin="$2"; shift 2
  log "  [lind]   $bin $* (x$RUNS)"
  run_multi "${tag}_lind" "$RUNS" timeout "$TIMEOUT_SEC" "$LIND_RUN" "/${bin}.opt.wasm" "$@"
}

run_lind_bash_multi() {
  local tag="$1" cmd="$2"; shift 2
  log "  [lind/bash] $cmd (x$RUNS)"
  run_multi "${tag}_lind" "$RUNS" timeout "$TIMEOUT_SEC" "$LIND_RUN" /bash.opt.wasm -c "$cmd"
}

ratio() {
  local n="$1" l="$2"
  if [ "$n" = "N/A" ] || [ "$l" = "N/A" ] || [ -z "$n" ] || [ -z "$l" ]; then
    echo "N/A"
    return
  fi
  awk "BEGIN { if ($n+0 == 0) print \"N/A\"; else printf \"%.2f\", $l / $n }" 2>/dev/null || echo "N/A"
}

# Format result line: | label | native_mean (n=X) | lind_mean (n=X) | overhead |
fmt_row() {
  local label="$1" nv="$2" nn="$3" lv="$4" ln="$5" unit="$6"
  local r
  r=$(ratio "$nv" "$lv")
  if [ "$r" != "N/A" ]; then r="${r}x"; fi
  local ndisp ldisp
  if [ "$nv" = "N/A" ]; then ndisp="N/A"; else ndisp="$nv (n=$nn)"; fi
  if [ "$lv" = "N/A" ]; then ldisp="N/A"; else ldisp="$lv (n=$ln)"; fi
  echo "| $label | $ndisp | $ldisp | $r |"
}

# Stage lind binaries
stage() {
  log "Staging lind binaries..."
  mkdir -p "$LINDFS/tmp" "$LINDFS/dev"
  for b in lat_ctx lat_syscall lat_sig lat_proc lat_pipe lat_unix lat_tcp bw_pipe bw_unix bw_tcp bw_mem hello bash msleep; do
    if [ -f "$LIND_BIN/${b}.opt.wasm" ]; then
      cp "$LIND_BIN/${b}.opt.wasm" "$LINDFS/"
    fi
  done
  # bash from apps
  local bash_wasm="$LIND_APPS_ROOT/build/bin/bash/wasm32-wasi/bash.opt.wasm"
  if [ -f "$bash_wasm" ]; then
    cp "$bash_wasm" "$LINDFS/"
  fi
  # files needed by benchmarks
  [ -f "$LINDFS/hello.opt.wasm" ] && cp "$LINDFS/hello.opt.wasm" "$LINDFS/tmp/hello"
  echo "test" > "$LINDFS/tmp/sigtest"
  if [ ! -e "$LINDFS/dev/null" ]; then mknod "$LINDFS/dev/null" c 1 3 2>/dev/null; chmod 666 "$LINDFS/dev/null" 2>/dev/null; fi
  if [ ! -e "$LINDFS/dev/zero" ]; then mknod "$LINDFS/dev/zero" c 1 5 2>/dev/null; chmod 666 "$LINDFS/dev/zero" 2>/dev/null; fi
  log "Staging done."
}

# Header
write_header() {
  {
    echo "# lmbench Benchmark Results"
    echo ""
    echo "**Date:** $(date '+%Y-%m-%d %H:%M:%S')"
    echo "**Host:** $(hostname)"
    echo "**Kernel:** $(uname -r)"
    echo "**CPU:** $(lscpu 2>/dev/null | awk -F: '/Model name/ {gsub(/^ +/,"",$2); print $2}' || echo 'N/A')"
    echo "**Compiler (native):** $(${CC:-gcc} --version 2>/dev/null | head -1 || echo 'N/A')"
    echo "**Optimization:** -O3 (native gcc), -O2 clang + wasm-opt (lind)"
    echo "**Repetitions per run:** $REPS | **Warmup:** $WARMUP | **Runs for mean:** $RUNS"
    echo ""
    echo "---"
    echo ""
  } > "$OUTFILE"
}

# ── Benchmarks ──

do_lat_syscall() {
  log "=== lat_syscall ==="
  {
    echo "## lat_syscall - System Call Latency (usec)"
    echo ""
    echo "| Syscall | Native | Lind | Overhead |"
    echo "|---------|--------|------|----------|"
  } >> "$OUTFILE"

  for mode in null read write stat fstat open; do
    local nv="N/A" nn=0 lv="N/A" ln=0
    local extra=""
    case "$mode" in stat|fstat|open) extra="/tmp/sigtest" ;; esac
    local nextra=""
    case "$mode" in stat|fstat|open) nextra="/tmp/sigtest_native" ;; esac

    if [ "$RUN_NATIVE" = "true" ]; then
      if [ -n "$nextra" ]; then
        read nv nn <<< $(run_native_multi "lat_syscall_$mode" lat_syscall -W $WARMUP -N $REPS $mode $nextra)
      else
        read nv nn <<< $(run_native_multi "lat_syscall_$mode" lat_syscall -W $WARMUP -N $REPS $mode)
      fi
    fi
    if [ "$RUN_LIND" = "true" ]; then
      if [ -n "$extra" ]; then
        read lv ln <<< $(run_lind_multi "lat_syscall_$mode" lat_syscall -W $WARMUP -N $REPS $mode $extra)
      else
        read lv ln <<< $(run_lind_multi "lat_syscall_$mode" lat_syscall -W $WARMUP -N $REPS $mode)
      fi
    fi
    fmt_row "$mode" "$nv" "$nn" "$lv" "$ln" "us" >> "$OUTFILE"
  done
  echo "" >> "$OUTFILE"
}

do_lat_sig() {
  log "=== lat_sig ==="
  {
    echo "## lat_sig - Signal Latency (usec)"
    echo ""
    echo "| Mode | Native | Lind | Overhead |"
    echo "|------|--------|------|----------|"
  } >> "$OUTFILE"

  # install
  local nv="N/A" nn=0 lv="N/A" ln=0
  if [ "$RUN_NATIVE" = "true" ]; then
    read nv nn <<< $(run_native_multi "lat_sig_install" lat_sig -W $WARMUP -N $REPS install)
  fi
  if [ "$RUN_LIND" = "true" ]; then
    read lv ln <<< $(run_lind_multi "lat_sig_install" lat_sig -W $WARMUP -N $REPS install)
  fi
  fmt_row "install" "$nv" "$nn" "$lv" "$ln" "us" >> "$OUTFILE"

  # catch (known N/A for lind - wasm trap)
  nv="N/A"; nn=0; lv="N/A"; ln=0
  if [ "$RUN_NATIVE" = "true" ]; then
    read nv nn <<< $(run_native_multi "lat_sig_catch" lat_sig -W $WARMUP -N $REPS catch)
  fi
  fmt_row "catch" "$nv" "$nn" "N/A" "0" "us" >> "$OUTFILE"

  # prot (known N/A for lind - wasm trap)
  echo "test" > /tmp/sigtest_native 2>/dev/null
  nv="N/A"; nn=0
  if [ "$RUN_NATIVE" = "true" ]; then
    read nv nn <<< $(run_native_multi "lat_sig_prot" lat_sig -W $WARMUP -N $REPS prot /tmp/sigtest_native)
  fi
  fmt_row "prot (fault)" "$nv" "$nn" "N/A" "0" "us" >> "$OUTFILE"

  echo "" >> "$OUTFILE"
  echo "*lat_sig catch/prot: N/A on lind - wasm trap: indirect call type mismatch (signal handler callback)*" >> "$OUTFILE"
  echo "" >> "$OUTFILE"
}

do_lat_proc() {
  log "=== lat_proc ==="
  {
    echo "## lat_proc - Process Creation Latency (usec)"
    echo ""
    echo "| Mode | Native | Lind | Overhead |"
    echo "|------|--------|------|----------|"
  } >> "$OUTFILE"

  for mode in fork exec; do
    local nv="N/A" nn=0 lv="N/A" ln=0
    if [ "$RUN_NATIVE" = "true" ]; then
      read nv nn <<< $(run_native_multi "lat_proc_$mode" lat_proc -W $WARMUP -N $REPS $mode)
    fi
    if [ "$RUN_LIND" = "true" ]; then
      read lv ln <<< $(run_lind_multi "lat_proc_$mode" lat_proc -W $WARMUP -N $REPS $mode)
    fi
    fmt_row "$mode" "$nv" "$nn" "$lv" "$ln" "us" >> "$OUTFILE"
  done
  echo "" >> "$OUTFILE"
}

do_lat_ctx() {
  log "=== lat_ctx ==="
  {
    echo "## lat_ctx - Context Switch Latency (usec)"
    echo ""
    echo "| Processes | Native | Lind | Overhead |"
    echo "|-----------|--------|------|----------|"
  } >> "$OUTFILE"

  for n in 2 4 8 16 32; do
    local nv="N/A" nn=0
    if [ "$RUN_NATIVE" = "true" ]; then
      read nv nn <<< $(run_native_multi "lat_ctx_$n" lat_ctx -W $WARMUP -N $REPS "$n")
    fi
    fmt_row "$n" "$nv" "$nn" "N/A" "0" "us" >> "$OUTFILE"
  done
  echo "" >> "$OUTFILE"
  echo "*lat_ctx: N/A on lind - panic: main threadid does not exist (signal state not initialized for forked processes)*" >> "$OUTFILE"
  echo "" >> "$OUTFILE"
}

do_lat_ipc() {
  log "=== lat_pipe / lat_unix ==="
  {
    echo "## IPC Latency (usec)"
    echo ""
    echo "| IPC | Native | Lind | Overhead |"
    echo "|-----|--------|------|----------|"
  } >> "$OUTFILE"

  for ipc in pipe unix; do
    local nv="N/A" nn=0 lv="N/A" ln=0
    if [ "$RUN_NATIVE" = "true" ]; then
      read nv nn <<< $(run_native_multi "lat_$ipc" "lat_$ipc" -W $WARMUP -N $REPS)
    fi
    if [ "$RUN_LIND" = "true" ]; then
      read lv ln <<< $(run_lind_multi "lat_$ipc" "lat_$ipc" -W $WARMUP -N $REPS)
    fi
    fmt_row "$ipc" "$nv" "$nn" "$lv" "$ln" "us" >> "$OUTFILE"
  done
  echo "" >> "$OUTFILE"
}

do_lat_tcp() {
  log "=== lat_tcp ==="
  {
    echo "## lat_tcp - TCP Latency (usec)"
    echo ""
    echo "| Mode | Native | Lind | Overhead |"
    echo "|------|--------|------|----------|"
  } >> "$OUTFILE"

  local nv="N/A" nn=0 lv="N/A" ln=0

  # Native: server+client
  if [ "$RUN_NATIVE" = "true" ]; then
    log "  [native] lat_tcp (x$RUNS)"
    local sum=0 count=0 i=1
    while [ "$i" -le "$RUNS" ]; do
      "$NATIVE_BIN/lat_tcp" -s &
      local spid=$!
      sleep 1
      local out
      out=$("$NATIVE_BIN/lat_tcp" -W $WARMUP -N $REPS localhost 2>&1) || true
      kill $spid 2>/dev/null; wait $spid 2>/dev/null
      echo "$out" >> "$RAWDIR/lat_tcp_native_run${i}.txt"
      local val
      val=$(parse_val "$out")
      if [ "$val" != "N/A" ] && [ -n "$val" ]; then
        sum=$(awk "BEGIN { printf \"%.6f\", $sum + $val }")
        count=$((count + 1))
      fi
      i=$((i + 1))
    done
    if [ "$count" -gt 0 ]; then
      nv=$(awk "BEGIN { printf \"%.4f\", $sum / $count }")
      nn=$count
    fi
  fi

  # Lind: bash approach
  if [ "$RUN_LIND" = "true" ]; then
    read lv ln <<< $(run_lind_bash_multi "lat_tcp" "lat_tcp.opt.wasm -s & msleep.opt.wasm 2000; lat_tcp.opt.wasm -P 1 127.0.0.1")
  fi

  fmt_row "tcp" "$nv" "$nn" "$lv" "$ln" "us" >> "$OUTFILE"
  echo "" >> "$OUTFILE"
}

do_bw_ipc() {
  log "=== bw_pipe / bw_unix ==="
  {
    echo "## IPC Bandwidth (MB/s)"
    echo ""
    echo "| IPC | Native | Lind | Ratio |"
    echo "|-----|--------|------|-------|"
  } >> "$OUTFILE"

  for ipc in pipe unix; do
    local nv="N/A" nn=0 lv="N/A" ln=0
    if [ "$RUN_NATIVE" = "true" ]; then
      read nv nn <<< $(run_native_multi "bw_$ipc" "bw_$ipc" -W $WARMUP -N $REPS)
    fi
    if [ "$RUN_LIND" = "true" ]; then
      read lv ln <<< $(run_lind_multi "bw_$ipc" "bw_$ipc" -W $WARMUP -N $REPS)
    fi
    fmt_row "$ipc" "$nv" "$nn" "$lv" "$ln" "MB/s" >> "$OUTFILE"
  done
  echo "" >> "$OUTFILE"
}

do_bw_tcp() {
  log "=== bw_tcp ==="
  {
    echo "## bw_tcp - TCP Bandwidth (MB/s)"
    echo ""
    echo "| Mode | Native | Lind | Ratio |"
    echo "|------|--------|------|-------|"
  } >> "$OUTFILE"

  local nv="N/A" nn=0 lv="N/A" ln=0

  # Native
  if [ "$RUN_NATIVE" = "true" ]; then
    log "  [native] bw_tcp (x$RUNS)"
    local sum=0 count=0 i=1
    while [ "$i" -le "$RUNS" ]; do
      "$NATIVE_BIN/bw_tcp" -s &
      local spid=$!
      sleep 1
      local out
      out=$("$NATIVE_BIN/bw_tcp" -W $WARMUP -N $REPS localhost 2>&1) || true
      kill $spid 2>/dev/null; wait $spid 2>/dev/null
      echo "$out" >> "$RAWDIR/bw_tcp_native_run${i}.txt"
      local val
      val=$(parse_val "$out")
      if [ "$val" != "N/A" ] && [ -n "$val" ]; then
        sum=$(awk "BEGIN { printf \"%.6f\", $sum + $val }")
        count=$((count + 1))
      fi
      i=$((i + 1))
    done
    if [ "$count" -gt 0 ]; then
      nv=$(awk "BEGIN { printf \"%.4f\", $sum / $count }")
      nn=$count
    fi
  fi

  # Lind: bash approach
  if [ "$RUN_LIND" = "true" ]; then
    read lv ln <<< $(run_lind_bash_multi "bw_tcp" "bw_tcp.opt.wasm -s & msleep.opt.wasm 2000; bw_tcp.opt.wasm 127.0.0.1")
  fi

  fmt_row "tcp" "$nv" "$nn" "$lv" "$ln" "MB/s" >> "$OUTFILE"
  echo "" >> "$OUTFILE"
}

do_bw_mem() {
  log "=== bw_mem rd ==="
  {
    echo "## bw_mem rd - Memory Read Bandwidth (MB/s)"
    echo ""
    echo "| Size | Native | Lind | Ratio |"
    echo "|------|--------|------|-------|"
  } >> "$OUTFILE"

  for sz in $BW_SIZES; do
    local h
    h=$(human_size $sz)
    local nv="N/A" nn=0 lv="N/A" ln=0

    if [ "$RUN_NATIVE" = "true" ]; then
      read nv nn <<< $(run_native_multi "bw_mem_$h" bw_mem -W $WARMUP -N $REPS $sz rd)
    fi
    if [ "$RUN_LIND" = "true" ]; then
      read lv ln <<< $(run_lind_multi "bw_mem_$h" bw_mem -W $WARMUP -N $REPS $sz rd)
    fi
    fmt_row "$h" "$nv" "$nn" "$lv" "$ln" "MB/s" >> "$OUTFILE"
  done
  echo "" >> "$OUTFILE"
}

# ── Main ──

log "=== lmbench Benchmark Suite ==="
log "Native: $RUN_NATIVE | Lind: $RUN_LIND | Runs: $RUNS"
log "Results: $OUTFILE"
log "Raw: $RAWDIR/"

echo "test" > /tmp/sigtest_native 2>/dev/null

if [ "$RUN_LIND" = "true" ]; then
  stage
fi

write_header

do_lat_syscall
log "--- lat_syscall done ---"

do_lat_sig
log "--- lat_sig done ---"

do_lat_proc
log "--- lat_proc done ---"

do_lat_ctx
log "--- lat_ctx done ---"

do_lat_ipc
log "--- lat_pipe/unix done ---"

do_lat_tcp
log "--- lat_tcp done ---"

do_bw_ipc
log "--- bw_pipe/unix done ---"

do_bw_tcp
log "--- bw_tcp done ---"

do_bw_mem
log "--- bw_mem done ---"

{
  echo "---"
  echo ""
  echo "*Generated by benchmark_lmbench.sh on $(date)*"
  echo "*Raw output: ${RAWDIR}/*"
} >> "$OUTFILE"

log ""
log "=== Done! ==="
log "Results: $OUTFILE"
