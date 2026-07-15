#!/bin/bash
# Compiles glibc's libm test sources (see gen_libm_tests.sh) into lind wasm targets.
# Pure compile+link+opt — does not run anything (see run_single_libm_test.sh /
# run_libm_tests.sh for that).
#
# Usage:
#   ./compile_libm_tests.sh                 # compile all in-scope tests (see
#                                            #   libm_common.sh's worklist() for scope)
#   ./compile_libm_tests.sh test-double-cos  # compile just one
#   LOG=1 ./compile_libm_tests.sh            # also write per-step logs to generated/obj/
#                                            #   (<test>.compile.log/.link.log/.opt.log)
#                                            #   instead of streaming to stdout/stderr —
#                                            #   for debugging a specific failure
#
# Output: generated/obj/<test>.wasm
set -uo pipefail

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/libm_common.sh"

ONLY="${1:-}"
LOG="${LOG:-0}"  # LOG=1: redirect each compile/link/opt step's output to a log file
                 # under generated/obj/ instead of stdout/stderr.

# run_step <logfile> <cmd...>: LOG=0 (default) lets output flow straight to this
# script's own stdout/stderr; LOG=1 appends it to <logfile> instead (append, since
# link.log is shared across several successive steps below — see start_log).
run_step() {
  local logfile="$1"; shift
  if [ "$LOG" = "1" ]; then
    "$@" >>"$logfile" 2>&1
  else
    "$@"
  fi
}

# start_log <logfile>: truncate <logfile> if LOG=1, so a re-run's log doesn't append
# onto a stale one from a previous (e.g. failed) attempt.
start_log() {
  [ "$LOG" = "1" ] && : > "$1"
}

[ -d "$CSRC" ] && ls "$CSRC"/test-*.c >/dev/null 2>&1 || { echo "error: no test sources in $CSRC — run ./gen_libm_tests.sh first" >&2; exit 1; }
[ -n "$LLVM" ] || { echo "error: could not find clang+llvm-* under $REPO_ROOT" >&2; exit 1; }

mkdir -p "$OBJ"

# support objects (compiled once per precision type, reused by every test of that type)
for t in double float ldouble; do
  [ -f "$OBJ/libm-test-support-$t.o" ] && continue
  run_step "$OBJ/support-$t.compile.log" \
    $CC $CFLAGS_INTERNAL $INCLUDES $SYSINC $DEFS -c "$CSRC/libm-test-support-$t.c" -o "$OBJ/libm-test-support-$t.o" \
    || echo "WARN: libm-test-support-$t.o failed to build" >&2
done

# argp_parse/argp_help stub (see argp_stubs.c) — libc.so doesn't export these, and
# every test binary references them via libm-test-support.c.
[ -f "$OBJ/argp_stubs.o" ] || $CC -c "$HERE/argp_stubs.c" -o "$OBJ/argp_stubs.o"

compile_one() {
  local name="$1" rettype="$2"
  local wasm="$OBJ/$name.wasm"
  [ -f "$wasm" ] && return 0

  local obj="$OBJ/$name.o"
  # NOTE: compiled with -fPIC (part of CFLAGS_INTERNAL, matching glibc's own build
  # config) so these .o files are already position-independent and link cleanly in
  # dynamic mode below.
  start_log "$OBJ/$name.compile.log"
  run_step "$OBJ/$name.compile.log" \
    $CC $CFLAGS_INTERNAL $INCLUDES $SYSINC $DEFS -c "$CSRC/$name.c" -o "$obj" \
    || { echo "COMPILE_FAIL: $name"; return 1; }

  # DYNAMIC link (matching lind_compile's do_dynamic_compile) — NOT static. The test
  # app must keep its libm calls as unresolved cross-module (GOT-indirect) references
  # so a grate can actually intercept them; a static link bakes libm in directly and a
  # grate has nothing to redirect at all — silently making any interposition
  # comparison meaningless. See ../LIBM_INTERPOSITION.md for how this was caught (via
  # `wasm-objdump -h`: no dylink.0 section, no "Needed" libm.so entry, when it should
  # be there).
  start_log "$OBJ/$name.link.log"
  run_step "$OBJ/$name.link.log" \
    clang --target=wasm32-unknown-wasi --sysroot="$SYSROOT" -pthread -fPIC \
      -fwasm-exceptions -mllvm -wasm-enable-sjlj \
      -nostartfiles \
      -Wl,-pie -Wl,--import-table -Wl,--import-memory -Wl,--export-memory \
      -Wl,--max-memory=67108864 -Wl,--allow-undefined \
      -Wl,--unresolved-symbols=import-dynamic \
      -Wl,--export=__wasm_call_ctors -Wl,--export-if-defined=__wasm_init_tls \
      -Wl,--export=__tls_base \
      "$obj" "$OBJ/libm-test-support-$rettype.o" "$OBJ/argp_stubs.o" \
      "$SYSROOT/lib/wasm32-wasi/crt1_shared.o" "$SYSROOT/lib/wasm32-wasi/lind_utils.o" \
      "$SYSROOT/lib/wasm32-wasi/lind_debug.o" -lm \
      -o "$wasm" || { echo "LINK_FAIL: $name"; rm -f "$obj"; return 1; }
  run_step "$OBJ/$name.link.log" "$REPO_ROOT/tools/add-export-tool/add-export-tool" "$wasm" "$wasm" __wasm_apply_tls_relocs func __wasm_apply_tls_relocs optional
  run_step "$OBJ/$name.link.log" "$REPO_ROOT/tools/add-export-tool/add-export-tool" "$wasm" "$wasm" __wasm_apply_global_relocs func __wasm_apply_global_relocs optional
  run_step "$OBJ/$name.link.log" "$REPO_ROOT/tools/add-export-tool/add-export-tool" "$wasm" "$wasm" __stack_pointer global __stack_pointer
  start_log "$OBJ/$name.opt.log"
  run_step "$OBJ/$name.opt.log" "$REPO_ROOT/scripts/lind-wasm-opt" --target=main "$wasm" -o "$wasm" \
    || { echo "OPT_FAIL: $name"; rm -f "$obj" "$wasm"; return 1; }
  rm -f "$obj"
}

n=0; fail=0
while read -r name rettype; do
  n=$((n+1))
  compile_one "$name" "$rettype" || fail=$((fail+1))
  [ $((n % 50)) -eq 0 ] && echo "progress: $n"
done < <(worklist "$ONLY")

echo ""
echo "=== done: $n tests, $fail failed to compile/link/opt ==="
echo "wasm in: $OBJ"
