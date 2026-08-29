#!/usr/bin/env bash
# Build the OpenBLAS *guest* module for shared-library mode: link our shim
# (openblas_lind_shim.c) into the OpenBLAS wasm dylink library and AOT-compile it to a
# .cwasm that SandboxedLib can load.
#
# Run this AFTER a successful compile_openblas.sh run (which produced libopenblas.a).
# It reuses the same toolchain compile_openblas.sh uses and mirrors its dylink→export→
# opt→precompile chain, adding the shim to the whole-archive link so guest_malloc /
# guest_free / lind_cblas_* become exports of the module.
#
#   ./build_guest.sh                       # uses the defaults below
#   OPENBLAS_A=/path/libopenblas.a OPENBLAS_SRC=/path/OpenBLAS ./build_guest.sh
#
# The few flag strings reconstructed from compile_openblas.sh are grouped at the top —
# if your compile_openblas.sh differs, make them match and re-run. Every step echoes
# its command, so a mismatch is easy to spot.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIM_C="$SCRIPT_DIR/openblas_lind_shim.c"

# --- locate lind-wasm and its toolchain (same conventions as compile_openblas.sh) ---
: "${LIND_WASM_ROOT:=$(cd "$SCRIPT_DIR/../../../.." && pwd)}"   # repo root: .../lind-wasm
: "${BASE_SYSROOT:=$LIND_WASM_ROOT/src/glibc/sysroot}"
: "${LLVM_BIN:=$(dirname "$(ls -d "$LIND_WASM_ROOT"/clang+llvm-*/bin/clang 2>/dev/null | head -n1)")}"
: "${LIND_WASM_OPT:=$LIND_WASM_ROOT/scripts/bin/lind-wasm-opt}"
: "${LIND_COMPILE:=$LIND_WASM_ROOT/scripts/lind_compile}"
: "${ADD_EXPORT_TOOL:=$LIND_WASM_ROOT/tools/add-export-tool/add-export-tool}"
CLANG="$LLVM_BIN/clang"

# --- inputs: the archive + headers your compile_openblas.sh produced -----------------
: "${OPENBLAS_SRC:=$SCRIPT_DIR/openblas}"          # OpenBLAS source tree (for cblas.h)
: "${OPENBLAS_A:=$OPENBLAS_SRC/libopenblas.a}"     # the static archive to whole-archive
: "${OUT_DIR:=$SCRIPT_DIR/build/guest}"
: "${LINDFS_DIR:=$LIND_WASM_ROOT/lindfs}"
: "${INSTALL_PATH:=$LINDFS_DIR/lib/libopenblas_lind.cwasm}"
: "${SKIP_ADD_EXPORT:=0}"                          # set 1 if opt/compile handle relocs

# --- flags reconstructed from compile_openblas.sh (CONFIRM these match) ---------------
CFLAGS=(-O2 -g -fPIC -fvisibility=default
        -fwasm-exceptions -mllvm -wasm-enable-sjlj
        -pthread -matomics -mbulk-memory)
LDFLAGS=(-Wl,--experimental-pic -Wl,-shared
         -Wl,--import-memory,--export-memory,--max-memory=67108864
         -Wl,--export=__stack_pointer,__stack_low,__tls_base)
RELOC_EXPORTS=(__wasm_apply_tls_relocs __wasm_apply_global_relocs)

# --- sanity checks -------------------------------------------------------------------
for f in "$SHIM_C" "$CLANG" "$OPENBLAS_A"; do
    [ -e "$f" ] || { echo "ERROR: missing: $f" >&2; exit 1; }
done
[ -e "$OPENBLAS_SRC/cblas.h" ] || echo "WARN: cblas.h not under OPENBLAS_SRC=$OPENBLAS_SRC (adjust -I below if the link fails)"
mkdir -p "$OUT_DIR"

WASM="$OUT_DIR/libopenblas_lind.wasm"
WASM_EXP="$OUT_DIR/libopenblas_lind.exp.wasm"
WASM_OPT="$OUT_DIR/libopenblas_lind.opt.wasm"

run() { echo "+ $*"; "$@"; }

# --- 1. link the shim into the OpenBLAS wasm dylink library --------------------------
# --whole-archive pulls in all of OpenBLAS so the shim's cblas_* calls resolve; the
# shim's export_name attributes make guest_malloc/guest_free/lind_cblas_* exports.
run "$CLANG" --target=wasm32-unknown-wasi --sysroot="$BASE_SYSROOT" \
    "${CFLAGS[@]}" \
    "$SHIM_C" -I"$OPENBLAS_SRC" \
    -Wl,--whole-archive "$OPENBLAS_A" -Wl,--no-whole-archive \
    "${LDFLAGS[@]}" \
    -o "$WASM"

# --- 2. add the dylink relocation exports (as the stock library build does) ----------
if [ "$SKIP_ADD_EXPORT" = "1" ]; then
    cp "$WASM" "$WASM_EXP"
else
    # NOTE: confirm this invocation matches how compile_openblas.sh calls add-export-tool.
    run "$ADD_EXPORT_TOOL" "$WASM" "$WASM_EXP" "${RELOC_EXPORTS[@]}"
fi

# --- 3. library-mode wasm opt --------------------------------------------------------
run "$LIND_WASM_OPT" --target=library "$WASM_EXP" -o "$WASM_OPT"

# --- 4. AOT-precompile to .cwasm -----------------------------------------------------
run "$LIND_COMPILE" --precompile-only "$WASM_OPT" -o "$OUT_DIR/libopenblas_lind.cwasm"

# --- 5. stage into lindfs ------------------------------------------------------------
mkdir -p "$(dirname "$INSTALL_PATH")"
run cp "$OUT_DIR/libopenblas_lind.cwasm" "$INSTALL_PATH"

echo
echo "guest module ready: $INSTALL_PATH"
echo "run:  make run GUEST_CWASM=$INSTALL_PATH"
