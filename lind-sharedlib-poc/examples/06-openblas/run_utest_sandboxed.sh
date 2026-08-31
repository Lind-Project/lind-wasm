#!/usr/bin/env bash
# Run OpenBLAS's utest suite against the SANDBOXED libopenblas.
#
# Unlike ctest (which calls the reference CBLAS `cblas_*`), utest calls the FORTRAN
# symbols via BLASFUNC (daxpy_, dscal_, ...). Our stub .so provides Fortran-ABI
# forwarders for the standard level-1 routines (see the "Fortran-ABI forwarders" section
# in stub/src/lib.rs); those deref their pointer args and call the sandboxed cblas_*.
#
# Loading model is identical to ctest (Option A): LIND_MODULE is the resident SHIM (main
# module); OpenBLAS is a PRELOAD.
#
# Like ctest, we RELINK the native build's utest objects against our stub .so FIRST (so it
# wins for every symbol we forward -> those route into the sandbox) then the native
# libopenblas.a (fills everything we don't forward yet -> runs natively). So the suite
# links and runs as a whole while the sandboxed set grows incrementally. Currently
# sandboxed: standard level-1 (axpy/copy/swap/scal/dot/nrm2/asum/iamax, real + complex).
# Still native (fallback): complex dot, dsdot, rotmg, gemv/gemm (Fortran char flags), and
# the OpenBLAS extensions (amax/amin/axpby/ismin).
#
# Prereq: build utest natively first so the .o files exist, e.g. in your native tree:
#     make -C utest        # compiles utest/*.o (its FINAL link may fail if the native
#                          # OpenBLAS lacks LAPACK — that's fine, we only need the .o's)
#
# A few test files test LAPACK (potrf/getrf/gesvd), not BLAS, and can't link against a
# LAPACK-less OpenBLAS (guest or native). They're out of scope here and are excluded from
# the relink (override with UTEST_EXCLUDE="a b c", base names without the .o).
#
#   OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS \
#   LIND_MODULE=$LIND_WASM_ROOT/lindfs/sharedlib-poc/openblas/openblas_lind_shim.cwasm \
#   OPENBLAS_CWASM=$LIND_WASM_ROOT/lindfs/lib/libopenblas.so \
#   ./run_utest_sandboxed.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

: "${OPENBLAS_NATIVE:?set OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS (with utest built)}"
: "${LIND_MODULE:?set LIND_MODULE=.../lindfs/sharedlib-poc/openblas/openblas_lind_shim.cwasm (the resident shim)}"
: "${OPENBLAS_CWASM:?set OPENBLAS_CWASM=.../lindfs/lib/libopenblas.so (plain OpenBLAS guest, preloaded)}"
STUB_DIR="${STUB_DIR:-$SCRIPT_DIR/stub/target/release}"
LINDFS_LIB="$(dirname "$OPENBLAS_CWASM")"
: "${LIND_PRELOAD:=env=$LINDFS_LIB/libc.cwasm,env=$LINDFS_LIB/libm.cwasm,env=$OPENBLAS_CWASM}"
# Engine fpcast OFF — OpenBLAS bakes its fpcast in at compile time (see ctest harness).
: "${LIND_ENABLE_FPCAST:=0}"

UT="$OPENBLAS_NATIVE/utest"
[ -f "$STUB_DIR/libopenblas.so" ] || { echo "build the stub first: (cd $SCRIPT_DIR && make host)"; exit 1; }
[ -d "$UT" ] || { echo "no utest dir at $UT — build it natively first (make -C utest)"; exit 1; }

nativea="$(ls "$OPENBLAS_NATIVE"/libopenblas*.a 2>/dev/null | head -n1 || true)"
[ -n "$nativea" ] || { echo "need native libopenblas.a in $OPENBLAS_NATIVE"; exit 1; }

# The main openblas_utest binary = utest_main.o + every test_*.o in utest/. Exclude
# utest_main2.o (that's the SEPARATE openblas_utest_ext main -> duplicate main), the
# test_extensions/ subdir (a separate binary, not yet wired), and the LAPACK-dependent
# tests (they reference potrf/getrf/gesvd, which neither the guest nor a LAPACK-less
# native .a provides). Each test_*.o self-registers with the ctest framework, so dropping
# one simply omits its tests — no dangling references from utest_main.o.
: "${UTEST_EXCLUDE:=test_potrs test_kernel_regress test_post_fork_async}"
objs=()
[ -f "$UT/utest_main.o" ] || { echo "no $UT/utest_main.o — run 'make -C utest' in $OPENBLAS_NATIVE first"; exit 1; }
objs+=("$UT/utest_main.o")
for o in "$UT"/test_*.o; do
    [ -e "$o" ] || continue
    base="$(basename "$o" .o)"
    case " $UTEST_EXCLUDE " in *" $base "*) echo "skip $base (excluded)"; continue ;; esac
    objs+=("$o")
done

out="$UT/openblas_utest_sandboxed"
echo "relink ${#objs[@]} utest objects -> $out  [stub .so wins for forwarded syms; native .a fills the rest]"
cc "${objs[@]}" -L"$STUB_DIR" -lopenblas "$nativea" -lm -lpthread -o "$out"

echo "=============== utest (SANDBOXED) ==============="
LIND_MODULE="$LIND_MODULE" LIND_PRELOAD="$LIND_PRELOAD" LIND_ENABLE_FPCAST="$LIND_ENABLE_FPCAST" \
  LD_LIBRARY_PATH="$STUB_DIR" "$out"

