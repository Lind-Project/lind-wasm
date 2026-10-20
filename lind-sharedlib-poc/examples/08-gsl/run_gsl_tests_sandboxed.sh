#!/usr/bin/env bash
# Validate the SANDBOXED GSL against GSL's own per-module test suite, module by module.
# Same model as 06-openblas's run_ctest_sandboxed.sh: compile a module's test driver
# natively, but LINK it so `gsl_*` / `cblas_*` resolve from our stub .so FIRST (→ they run
# inside the lind sandbox) and the native .a only fills whatever we haven't wrapped.
#
# This example (the wrappers + harness) lives in lind-sharedlib-poc; the GSL SOURCE lives
# separately in lind-wasm-apps/gsl-2.8, so point GSL_SRC at it (for the module test.c files
# and the test/ harness) and GSL_NATIVE at a plain native GSL build (headers + fill archives).
#
# Prereqs:
#   1. GSL source tree (has <module>/test.c, test/results.c)       -> GSL_SRC
#   2. A NATIVE GSL build (plain ./configure && make)              -> GSL_NATIVE
#   3. Sandboxed GSL built by compile_gsl.sh                        -> GSL_CWASM, LIND_MODULE
#   4. The GSL stub cdylib (libgsl.so) — `make host` here          -> STUB_DIR
#
# Usage:
#   GSL_SRC=/path/to/lind-wasm-apps/gsl-2.8 \
#   GSL_NATIVE=/path/to/native-gsl-build \
#   LIND_MODULE=/abs/lindfs/sharedlib-poc/gsl/gsl_lind_shim.cwasm \
#   GSL_CWASM=/abs/lindfs/lib/libgsl.so \
#   STUB_DIR=./stub/target/release \
#   ./run_gsl_tests_sandboxed.sh [module ...]        # default: a starter set below
#
#   NATIVE_BASELINE=1 ...   link WITHOUT the stub (pure native) — a control run.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

: "${GSL_SRC:?set GSL_SRC=/path/to/lind-wasm-apps/gsl-2.8 (the GSL source tree)}"
: "${GSL_NATIVE:?set GSL_NATIVE=/path/to/a native GSL build (plain ./configure && make)}"
: "${STUB_DIR:=$HERE/stub/target/release}"
: "${LIND_MODULE:?set LIND_MODULE=.../sharedlib-poc/gsl/gsl_lind_shim.cwasm (the resident shim)}"
: "${GSL_CWASM:?set GSL_CWASM=.../lindfs/lib/libgsl.so (sandboxed GSL guest, preloaded)}"

CC="${CC:-cc}"
LINDFS_LIB="$(dirname "$GSL_CWASM")"
: "${LIND_PRELOAD:=env=$LINDFS_LIB/libc.cwasm,env=$LINDFS_LIB/libm.cwasm,env=$GSL_CWASM}"
: "${LIND_ENABLE_FPCAST:=0}"
: "${NATIVE_BASELINE:=0}"

GSL_A="$(ls "$GSL_NATIVE"/.libs/libgsl.a 2>/dev/null || true)"
CBLAS_A="$(ls "$GSL_NATIVE"/cblas/.libs/libgslcblas.a 2>/dev/null || true)"
[[ -f "$GSL_A" ]]   || { echo "no native libgsl.a under $GSL_NATIVE/.libs — build native GSL first"; exit 2; }
[[ -f "$CBLAS_A" ]] || { echo "no native libgslcblas.a under $GSL_NATIVE/cblas/.libs"; exit 2; }
[[ "$NATIVE_BASELINE" == 1 || -f "$STUB_DIR/libgsl.so" ]] || { echo "build the stub first: make host"; exit 2; }

HARNESS_SRC="$GSL_SRC/test/results.c"
[[ -f "$HARNESS_SRC" ]] || { echo "GSL test harness not found at $HARNESS_SRC (check GSL_SRC)"; exit 2; }

# -I the native build dir so assembled <gsl/gsl_xxx.h> resolve, plus the source root.
INCLUDES=(-I"$GSL_NATIVE" -I"$GSL_SRC")

MODULES=("$@")
if [[ ${#MODULES[@]} -eq 0 ]]; then
    MODULES=(poly blas vector matrix block sort statistics permutation sys const)
fi

BUILD="$HERE/build-tests"; mkdir -p "$BUILD"
run_lind() { LIND_MODULE="$LIND_MODULE" LIND_PRELOAD="$LIND_PRELOAD" \
    LIND_ENABLE_FPCAST="$LIND_ENABLE_FPCAST" LD_LIBRARY_PATH="$STUB_DIR" "$@"; }

# A module's driver is almost always JUST test.c. Do NOT glob test_*.c — in GSL those
# are usually one of:
#   - template bodies #included by test.c with macros predefined (block/test_io.c,
#     sort/test_heapsort.c) — they have no headers and won't compile standalone;
#   - alternative drivers that ALSO define main (vector/test_static.c, matrix/test_static.c).
# Compiling either breaks the link. Only a FEW modules genuinely split their driver across
# separately-compiled files; list those explicitly here (companions beyond test.c).
module_companions() {
    case "$1" in
        statistics) echo "test_nist.c test_robust.c ttest.c" ;;
        *) echo "" ;;
    esac
}
module_sources() {
    local m="$1" c
    echo "$GSL_SRC/$m/test.c"
    for c in $(module_companions "$m"); do
        [[ -e "$GSL_SRC/$m/$c" ]] && echo "$GSL_SRC/$m/$c"
    done
}

pass=0; fail=0; skip=0
for m in "${MODULES[@]}"; do
    tsrc="$GSL_SRC/$m/test.c"
    if [[ ! -f "$tsrc" ]]; then echo "skip $m (no $m/test.c)"; skip=$((skip+1)); continue; fi
    mapfile -t SRCS < <(module_sources "$m")
    out="$BUILD/test_$m"
    if [[ "$NATIVE_BASELINE" == 1 ]]; then
        echo "=== [$m] NATIVE baseline ==="
        "$CC" "${INCLUDES[@]}" "${SRCS[@]}" "$HARNESS_SRC" "$GSL_A" "$CBLAS_A" -lm -o "$out" \
            || { echo "  link FAILED"; fail=$((fail+1)); continue; }
        "$out" && { echo "  [$m] PASS (native)"; pass=$((pass+1)); } || { echo "  [$m] FAIL (native)"; fail=$((fail+1)); }
    else
        echo "=== [$m] SANDBOXED (stub wins -> lind) ==="
        "$CC" "${INCLUDES[@]}" "${SRCS[@]}" "$HARNESS_SRC" \
            -L"$STUB_DIR" -lgsl "$GSL_A" "$CBLAS_A" -lm -o "$out" \
            || { echo "  link FAILED"; fail=$((fail+1)); continue; }
        if run_lind "$out"; then echo "  [$m] PASS (sandboxed)"; pass=$((pass+1));
        else echo "  [$m] FAIL (sandboxed)"; fail=$((fail+1)); fi
    fi
done

echo
echo "RESULTS: modules ok=$pass fail=$fail skipped=$skip"
