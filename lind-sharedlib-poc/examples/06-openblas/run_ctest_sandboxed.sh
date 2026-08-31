#!/usr/bin/env bash
# Run the reference CBLAS drivers against the SANDBOXED libopenblas.so.
#
# It reuses the object files your native `make ctest` already built (so all the
# OpenBLAS-internal headers/config.h are already resolved) and RELINKS them against
# our native stub .so — so cblas_* calls route into the wasm sandbox.
#
#   OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS \
#   LIND_MODULE=$LIND_WASM_ROOT/lindfs/lib/libopenblas.so \
#   ./run_ctest_sandboxed.sh
#
# Config (env overrides):
#   LEVELS="1 2"        which BLAS levels to run
#   PRECISIONS="s d"    which precisions
#   LEVEL2_ROUTINES="gemv"   which level-2 routines to enable (must be wrapped in our .so)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

: "${OPENBLAS_NATIVE:?set OPENBLAS_NATIVE=/path/to/openblas-native/OpenBLAS (your native build)}"
: "${LIND_MODULE:?set LIND_MODULE=/path/to/lindfs/lib/libopenblas.so (the guest .cwasm)}"
STUB_DIR="${STUB_DIR:-$SCRIPT_DIR/stub/target/release}"
: "${LIND_PRELOAD:=env=$(dirname "$LIND_MODULE")/libc.cwasm,env=$(dirname "$LIND_MODULE")/libm.cwasm}"
# OpenBLAS's fpcast is baked in at compile time (self-contained trampolines), so the
# ENGINE fpcast must be OFF — turning it on double-applies it and traps "indirect call
# type mismatch" on level-2+ indirect kernel dispatch.
: "${LIND_ENABLE_FPCAST:=0}"
: "${LEVELS:=1 2 3}"
: "${PRECISIONS:=s d}"
# Per-level precision sets. Complex (c/z) is wired for the levels whose complex routines
# exist in the stub — currently level 1. As complex level-2/3 land, add c z here.
: "${L1_PRECISIONS:=s d c z}"
: "${L2_PRECISIONS:=s d c z}"
: "${L3_PRECISIONS:=$PRECISIONS}"
# Union of real + complex level-2 base names. Each precision only enables the ones that
# appear in its own ?in2 (e.g. symv exists for s/d, hemv/geru/gerc/her*/hpr* for c/z), so
# a shared superset is safe — un-present names simply never match.
: "${LEVEL2_ROUTINES:=gemv gbmv symv sbmv spmv hemv hbmv hpmv trmv tbmv tpmv trsv tbsv tpsv ger geru gerc syr spr her hpr syr2 spr2 her2 hpr2}"
: "${LEVEL3_ROUTINES:=gemm symm syrk syr2k trmm trsm}"

CT="$OPENBLAS_NATIVE/ctest"
[ -f "$STUB_DIR/libopenblas.so" ] || { echo "build the stub first: (cd $SCRIPT_DIR && make host)"; exit 1; }

runit() { # $1 = executable, rest = stdin redirect handled by caller
    LIND_MODULE="$LIND_MODULE" LIND_PRELOAD="$LIND_PRELOAD" LIND_ENABLE_FPCAST="$LIND_ENABLE_FPCAST" \
      LD_LIBRARY_PATH="$STUB_DIR" "$@"
}

# Pick the driver object for precision $1, level $2 (Fortran c_?blat?.o preferred, else
# the f2c C driver c_?blat?c.o). Echoes the path, empty if none.
driver_obj() { ls "$CT/c_$1blat$2.o" "$CT/c_$1blat${2}c.o" 2>/dev/null | head -n1 || true; }

# Choose linker (gfortran for a Fortran driver object, else cc).
linker_for() { case "$1" in *c_?blat?.o) echo gfortran ;; *) echo cc ;; esac; }

# --- level 1: self-contained (no input file) ---------------------------------------
run_level1() {
    local p="$1" wrap="$CT/c_${p}blas1.o" driver out nativea
    [ -f "$wrap" ] || { echo "skip L1 ${p}: missing $wrap"; return; }
    driver="$(driver_obj "$p" 1)"; [ -n "$driver" ] || { echo "skip L1 ${p}: no driver"; return; }
    # Native .a fills any symbol the wrapper references but we don't wrap (e.g. the
    # complex driver's Fortran refs); wins nothing our .so already provides.
    nativea="$(ls "$OPENBLAS_NATIVE"/libopenblas*.a 2>/dev/null | head -n1 || true)"
    out="$CT/x${p}cblat1_sandboxed"
    echo "relink $(basename "$driver") + c_${p}blas1.o -> $out"
    "$(linker_for "$driver")" "$driver" "$wrap" -L"$STUB_DIR" -lopenblas ${nativea:+"$nativea"} -lm -o "$out"
    echo "=============== ctest level-1 ${p} (SANDBOXED) ==============="
    runit "$out"
}

# --- levels 2 & 3: read ?in2/?in3; need support objs + native .a for un-wrapped syms --
# The wrapper c_?blasL.o references EVERY level-L cblas_*; we only wrap some. Link our
# .so first (wins for what we wrapped) then the native libopenblas.a (fills the rest —
# never called because the generated input disables them). Error-exit tests are disabled
# too (they need the xerbla callback we haven't built).
gen_input() { # $1 = source ?inL, $2 = precision, $3 = enabled base names
    awk -v p="$2" -v en="$3" '
        BEGIN { n=split(en, a, " "); for (i=1;i<=n;i++) on["cblas_" p a[i]]=1 }
        /LOGICAL FLAG, T TO TEST ERROR EXITS/ { sub(/^[[:space:]]*T/, "F"); print; next }
        /^cblas_/ { f = ($1 in on) ? "T" : "F"; sub(/^cblas_[a-z0-9]+[[:space:]]+[TF]/, $1 "  " f); print; next }
        { print }
    ' "$1"
}

# Run one matrix-level driver (level 2 or 3) for precision $1, level $2, routines $3.
run_matrix_level() {
    local p="$1" lvl="$2" routines="$3"
    local wrap="$CT/c_${p}blas${lvl}.o" driver out nativea input support=()
    [ -f "$wrap" ] || { echo "skip L${lvl} ${p}: missing $wrap"; return; }
    driver="$(driver_obj "$p" "$lvl")"; [ -n "$driver" ] || { echo "skip L${lvl} ${p}: no driver"; return; }
    nativea="$(ls "$OPENBLAS_NATIVE"/libopenblas*.a 2>/dev/null | head -n1 || true)"
    [ -n "$nativea" ] || { echo "skip L${lvl} ${p}: need native libopenblas.a in $OPENBLAS_NATIVE"; return; }
    # Match the native xN?cblatL link set: driver + wrappers + the error-exit checker
    # c_?Lchke.o (also DEFINES the cblas_ok/lerr/info/rout globals c_xerbla.o uses) +
    # auxiliary + c_xerbla + constant. Linked so symbols resolve; the chke path stays
    # dormant because the generated input disables error-exits.
    local o
    for o in "c_${p}${lvl}chke.o" auxiliary.o constant.o c_xerbla.o; do
        [ -f "$CT/$o" ] && support+=("$CT/$o")
    done

    out="$CT/x${p}cblat${lvl}_sandboxed"
    echo "relink $(basename "$driver") + c_${p}blas${lvl}.o (+support) -> $out  [+native .a fills un-wrapped]"
    "$(linker_for "$driver")" "$driver" "$wrap" "${support[@]}" \
        -L"$STUB_DIR" -lopenblas "$nativea" -lm -o "$out"

    input="$(mktemp)"; gen_input "$CT/${p}in${lvl}" "$p" "$routines" > "$input"
    echo "=========== ctest level-${lvl} ${p} (SANDBOXED; routines: $routines) ==========="
    runit "$out" < "$input"
    rm -f "$input"
}

for lvl in $LEVELS; do
    case "$lvl" in
        1) for p in $L1_PRECISIONS; do run_level1 "$p"; done ;;
        2) for p in $L2_PRECISIONS; do run_matrix_level "$p" 2 "$LEVEL2_ROUTINES"; done ;;
        3) for p in $L3_PRECISIONS; do run_matrix_level "$p" 3 "$LEVEL3_ROUTINES"; done ;;
        *) echo "level $lvl not supported yet" ;;
    esac
done
