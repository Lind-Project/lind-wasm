#!/usr/bin/env bash
# Build bench_openblas.c twice and run it: once NATIVE (native libopenblas.a) and once
# SANDBOXED (our stub .so -> wasm OpenBLAS in the lind sandbox). Same source, same sizes;
# only the cblas_* provider differs. Mirrors the link model of run_ctest_sandboxed.sh.
#
# Prereqs: `make host` (stub .so) and the shim/guest built (make shim) and staged in lindfs.
#
#   OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS \
#   LIND_MODULE=/abs/path/lindfs/openblas/openblas_lind_shim.cwasm \
#   OPENBLAS_CWASM=/abs/path/lindfs/lib/libopenblas.so \
#   ./run_bench_sandboxed.sh
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

: "${OPENBLAS_NATIVE:?set OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS}"
: "${LIND_MODULE:?set LIND_MODULE=/abs/lindfs/openblas/openblas_lind_shim.cwasm}"
: "${OPENBLAS_CWASM:?set OPENBLAS_CWASM=/abs/lindfs/lib/libopenblas.so}"
STUB_DIR="${STUB_DIR:-$SCRIPT_DIR/stub/target/release}"
LINDFS_LIB="$(dirname "$OPENBLAS_CWASM")"
: "${LIND_PRELOAD:=env=$LINDFS_LIB/libc.cwasm,env=$LINDFS_LIB/libm.cwasm,env=$OPENBLAS_CWASM}"
: "${LIND_ENABLE_FPCAST:=0}"
# Single-threaded native for an apples-to-apples call (the sandbox path is serialized).
# Drop this line (or set >1) to compare against multi-threaded native as well.
: "${OPENBLAS_NUM_THREADS:=1}"; export OPENBLAS_NUM_THREADS OMP_NUM_THREADS="$OPENBLAS_NUM_THREADS"

nativea="$(ls "$OPENBLAS_NATIVE"/libopenblas*.a 2>/dev/null | head -n1 || true)"
[ -n "$nativea" ] || { echo "need native libopenblas.a in $OPENBLAS_NATIVE"; exit 1; }
[ -f "$STUB_DIR/libopenblas.so" ] || { echo "build the stub first: (cd $SCRIPT_DIR && make host)"; exit 1; }

BUILD="$SCRIPT_DIR/build"; mkdir -p "$BUILD"
src="$SCRIPT_DIR/bench_openblas.c"
# BENCH_CSV=1 -> emit machine-readable rows to build/bench_{native,sandbox}.csv for charting.
: "${BENCH_CSV:=0}"; export BENCH_CSV

echo "=============== NATIVE (OPENBLAS_NUM_THREADS=$OPENBLAS_NUM_THREADS) ==============="
cc -O2 "$src" "$nativea" -lm -lpthread -o "$BUILD/bench_native"
if [ "$BENCH_CSV" = 1 ]; then BENCH_IMPL=native "$BUILD/bench_native" | tee "$BUILD/bench_native.csv"
else BENCH_IMPL=native "$BUILD/bench_native"; fi

echo
echo "=============== SANDBOXED (lind) ==============="
# stub .so wins for cblas_*; native .a fills anything unwrapped; run under the lind env.
cc -O2 "$src" -L"$STUB_DIR" -lopenblas "$nativea" -lm -lpthread -o "$BUILD/bench_sandbox"
run_sbx(){ LIND_MODULE="$LIND_MODULE" LIND_PRELOAD="$LIND_PRELOAD" LIND_ENABLE_FPCAST="$LIND_ENABLE_FPCAST" \
           BENCH_IMPL=sandbox LD_LIBRARY_PATH="$STUB_DIR" "$BUILD/bench_sandbox"; }
if [ "$BENCH_CSV" = 1 ]; then run_sbx | tee "$BUILD/bench_sandbox.csv"
    echo; echo "CSV written: $BUILD/bench_native.csv , $BUILD/bench_sandbox.csv"
else run_sbx; fi
