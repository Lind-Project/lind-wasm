#!/usr/bin/env bash
# Run OpenBLAS's test_extensions/ suite (the ~600 "utest_ext" tests) against the
# SANDBOXED libopenblas — the sibling of run_utest_sandboxed.sh for the extension API
# (axpby, geadd, gemmt, imatcopy/omatcopy, sum, axpyc, complex rotg, index-of-min-abs, ...).
#
# Same model as the base utest harness: relink the native ext objects against our stub .so
# FIRST (wins -> sandbox for every symbol we forward) then the native libopenblas.a (fills
# the rest -> native). The ext binary = utest_main.o + test_extensions/{xerbla,common}.o +
# test_extensions/test_*.o (see the OpenBLAS utest Makefile: OBJS_EXT).
#
# What's already sandboxed for free (routines we forwarded for ctest/base-utest): axpby,
# gemm, gemv, scal, amax, samin/damin/scamin/dzamin, csrot/zdrot, rotmg. Everything else in
# test_extensions is a NEW routine (geadd/gemmt/imatcopy/omatcopy/sum/axpyc/rotg/icamin/...)
# and runs on the native fallback until wrapped.
#
# Prereq: compile the ext objects natively. `make -C utest` stops at the LAPACK-less
# openblas_utest link, so use keep-going to build ALL objects (both binaries'), ignoring
# link failures:
#     make -C utest -k        # compiles utest/*.o and utest/test_extensions/*.o
#
#   OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS \
#   LIND_MODULE=.../openblas_lind_shim.cwasm OPENBLAS_CWASM=.../libopenblas.so \
#   ./run_utest_ext_sandboxed.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

: "${OPENBLAS_NATIVE:?set OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS (with test_extensions built)}"
: "${LIND_MODULE:?set LIND_MODULE=.../openblas_lind_shim.cwasm (the resident shim)}"
: "${OPENBLAS_CWASM:?set OPENBLAS_CWASM=.../libopenblas.so (plain OpenBLAS guest, preloaded)}"
STUB_DIR="${STUB_DIR:-$SCRIPT_DIR/stub/target/release}"
LINDFS_LIB="$(dirname "$OPENBLAS_CWASM")"
: "${LIND_PRELOAD:=env=$LINDFS_LIB/libc.cwasm,env=$LINDFS_LIB/libm.cwasm,env=$OPENBLAS_CWASM}"
: "${LIND_ENABLE_FPCAST:=0}"

UT="$OPENBLAS_NATIVE/utest"
EXT="$UT/test_extensions"
[ -f "$STUB_DIR/libopenblas.so" ] || { echo "build the stub first: (cd $SCRIPT_DIR && make host)"; exit 1; }
[ -d "$EXT" ] || { echo "no test_extensions dir at $EXT — build it natively first (make -C utest -k)"; exit 1; }
[ -f "$UT/utest_main.o" ] || { echo "no $UT/utest_main.o — run 'make -C utest -k' in $OPENBLAS_NATIVE first"; exit 1; }

nativea="$(ls "$OPENBLAS_NATIVE"/libopenblas*.a 2>/dev/null | head -n1 || true)"
[ -n "$nativea" ] || { echo "need native libopenblas.a in $OPENBLAS_NATIVE"; exit 1; }

# Excluded by default (override UTEST_EXT_EXCLUDE="a b c"):
#  - test_bgemm: bfloat16 (sbgemm) — the guest was built BUILD_BFLOAT16=0.
#  - test_cspmv / test_zspmv: complex-SYMMETRIC packed mat-vec (cspmv_/zspmv_) are LAPACK
#    AUXILIARY routines, absent from a NO_LAPACK build — neither guest nor native has them.
: "${UTEST_EXT_EXCLUDE:=test_bgemm test_cspmv test_zspmv}"


objs=("$UT/utest_main.o")
for o in xerbla.o common.o; do [ -f "$EXT/$o" ] && objs+=("$EXT/$o"); done
for o in "$EXT"/test_*.o; do
    [ -e "$o" ] || continue
    base="$(basename "$o" .o)"
    case " $UTEST_EXT_EXCLUDE " in *" $base "*) echo "skip $base (excluded)"; continue ;; esac
    objs+=("$o")
done

# NATIVE_BASELINE=1 links WITHOUT our stub .so (pure native libopenblas.a) — a control run
# to separate failures the sandbox causes from ones that already fail natively on this
# build. Nothing is sandboxed in this mode.
if [ "${NATIVE_BASELINE:-0}" = "1" ]; then
    out="$EXT/openblas_utest_ext_native"
    echo "relink ${#objs[@]} ext objects -> $out  [NATIVE BASELINE: no stub .so, all native]"
    cc "${objs[@]}" "$nativea" -lm -lpthread -o "$out"
    echo "=============== utest_ext (NATIVE BASELINE) ==============="
    "$out"
    exit $?
fi

out="$EXT/openblas_utest_ext_sandboxed"
echo "relink ${#objs[@]} ext objects -> $out  [stub .so wins for forwarded syms; native .a fills the rest]"
cc "${objs[@]}" -L"$STUB_DIR" -lopenblas "$nativea" -lm -lpthread -o "$out"

echo "=============== utest_ext (SANDBOXED) ==============="
LIND_MODULE="$LIND_MODULE" LIND_PRELOAD="$LIND_PRELOAD" LIND_ENABLE_FPCAST="$LIND_ENABLE_FPCAST" \
  LD_LIBRARY_PATH="$STUB_DIR" "$out"

