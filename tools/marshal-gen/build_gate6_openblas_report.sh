#!/usr/bin/env bash
# Runs the real, unmodified OpenBLAS test suite (openblas_utest,
# openblas_utest_ext) against Gate 4's real V2 grate, staged the way the
# plan's own Gate 6 text asks for -- issue #22's OpenBLAS inference-to-
# runtime integration, Gate 6: "staged real OpenBLAS testing".
#
# Reproduces three things per binary, with every raw log preserved on
# disk (not just summarized in prose): the existing non-interposed
# baseline; a strict `:interposed` run (fails closed -- traps the whole
# process on the first symbol the grate doesn't register, by design);
# and a mixed run (a registered symbol dispatches through the grate, an
# unregistered one falls straight through to the real library instead of
# trapping, so a run can reach past a coverage gap and still produce
# real per-symbol trace evidence for everything up to its own first
# genuine crash). tools/marshal-gen/gate6_report.py then derives the
# taxonomy/symbol-coverage report from these same raw logs via Gate 5's
# execution_oracle.py, mechanically, not by hand.
#
# Like build_openblas_v2_grate.sh, the manifest/verified marshal.json/
# grate this script builds along the way are intermediate and are not
# kept -- they are fully reproducible from openblas-inference/final/
# openblas_inference.json + llm-prompts/openblas-v6-full + the real
# archive. Only $OUT_DIR (raw logs + gate6_report.json) is the actual
# deliverable and is left in place.
#
# Usage:
#   tools/marshal-gen/build_gate6_openblas_report.sh <out-dir> [<libopenblas.a path>]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

OUT_DIR="${1:?usage: $0 <out-dir> [<libopenblas.a path>]}"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"  # absolute: step 4 below cd's into $LINDFS first
ARTIFACT="$REPO_ROOT/openblas-inference/final/openblas_inference.json"
PROMPTS_DIR="$REPO_ROOT/llm-prompts/openblas-v6-full"
ARCHIVE="${2:-$(cd "$REPO_ROOT/.." 2>/dev/null && pwd)/lind-wasm-apps/openblas/libopenblas.a}"
LINDFS="$REPO_ROOT/lindfs"
LIND_RUN="$REPO_ROOT/scripts/bin/lind_run"

if [[ ! -f "$ARTIFACT" ]]; then
    echo "error: frozen artifact not found at $ARTIFACT" >&2
    exit 1
fi
if [[ ! -f "$ARCHIVE" ]]; then
    echo "error: real libopenblas.a not found at $ARCHIVE" >&2
    exit 1
fi
for bin in openblas_utest openblas_utest_ext; do
    if [[ ! -f "$LINDFS/usr/local/bin/$bin" ]]; then
        echo "error: $bin not staged at $LINDFS/usr/local/bin/$bin (build/install openblas first)" >&2
        exit 1
    fi
done

WORK="$(mktemp -d)"
GRATE_SRC="$REPO_ROOT/tests/grate-tests/lib-interpose/libopenblas_v2_grate_gate6.c"
cleanup() {
    rm -rf "$WORK" "$GRATE_SRC" "${GRATE_SRC%.c}.wasm" "${GRATE_SRC%.c}.cwasm" \
           "$LINDFS/grates/libopenblas_v2_grate_gate6.cwasm"
}
trap cleanup EXIT

MANIFEST="$OUT_DIR/gate6_manifest.json"
VERIFIED_MARSHAL="$WORK/openblas_verified.marshal.json"

echo "[gate6] step 1/4: building the manifest + verified marshal.json"
python3 "$SCRIPT_DIR/gate4_manifest.py" "$ARTIFACT" \
    --prompts-dir "$PROMPTS_DIR" --archive "$ARCHIVE" \
    --out-manifest "$MANIFEST" --out-verified-marshal "$VERIFIED_MARSHAL"
echo ""

echo "[gate6] step 2/4: generating + compiling the grate with -DLIND_MARSHAL_DEBUG"
python3 "$SCRIPT_DIR/gen_v2_adapter.py" "$VERIFIED_MARSHAL" \
    --lib-name libopenblas --out "$GRATE_SRC" \
    --emit-grate --allow-partial
"$REPO_ROOT/scripts/lind_compile" -s --compile-grate "$GRATE_SRC" \
    -I "$REPO_ROOT/tests/grate-tests/lib-interpose" -DLIND_MARSHAL_DEBUG "$ARCHIVE"
mkdir -p "$LINDFS/grates"
cp "${GRATE_SRC%.c}.cwasm" "$LINDFS/grates/"
echo ""

# Runs one captured command, writing its combined stdout+stderr to
# $OUT_DIR/<name>.log and its REAL exit code to $OUT_DIR/<name>.log.exit
# -- gate6_report.py reads both. A nonzero exit (a failing ctest tally, or
# a deliberate strict-mode trap) is an expected, real RESULT to preserve,
# never a reason for this driver itself to abort -- but it must be the
# command's OWN status, not `true`'s: `set +e` around the single command
# (rather than `cmd || true`, which replaces a failing command's exit
# code with 0 -- a real bug caught by review, since every downstream
# .exit file would otherwise always read 0, including for a run that
# actually trapped) captures it before `set -e` resumes.
run_capture() {
    local name="$1"; shift
    local log="$OUT_DIR/$name.log"
    local status
    set +e
    "$@" >"$log" 2>&1
    status=$?
    set -e
    echo "$status" >"$log.exit"
    echo "[gate6]   $name: exit=$status $(grep -o 'RESULTS: [0-9]* tests ([^)]*)' "$log" || echo '(no RESULTS line)')"
}

# Self-test run_capture's own exit-code fidelity on every invocation of
# this script, not just once in a separate test file -- a regression
# here would otherwise silently corrupt every .exit file this script
# writes (see the comment above run_capture's own definition).
run_capture "_selftest_nonzero" bash -c 'echo probe; exit 7'
_selftest_status="$(cat "$OUT_DIR/_selftest_nonzero.log.exit")"
if [[ "$_selftest_status" != "7" ]]; then
    echo "error: run_capture self-test failed -- expected exit 7, got $_selftest_status" >&2
    exit 1
fi
rm -f "$OUT_DIR/_selftest_nonzero.log" "$OUT_DIR/_selftest_nonzero.log.exit"

PRELOAD_ARGS=()
if [[ -f "$LINDFS/lib/libopenblas.so" ]]; then
    PRELOAD_ARGS=(--preload env=lib/libopenblas.so)
fi

echo "[gate6] step 3/4: baseline (non-interposed, matches lind-wasm-apps/openblas/run_tests.sh)"
for bin in openblas_utest openblas_utest_ext; do
    run_capture "baseline_${bin#openblas_}" \
        sudo "$LIND_RUN" "${PRELOAD_ARGS[@]}" "/usr/local/bin/$bin"
done
echo ""

echo "[gate6] step 4/4: interposed runs against the real grate (strict, then mixed)"
pushd "$LINDFS" >/dev/null
for bin in openblas_utest openblas_utest_ext; do
    run_capture "strict_${bin#openblas_}" \
        sudo timeout 120 "$LIND_RUN" --preload env=lib/libopenblas.so:interposed \
        grates/libopenblas_v2_grate_gate6.cwasm "/usr/local/bin/$bin"
    run_capture "mixed_${bin#openblas_}" \
        sudo timeout 180 "$LIND_RUN" --preload env=lib/libopenblas.so \
        grates/libopenblas_v2_grate_gate6.cwasm "/usr/local/bin/$bin"
done
popd >/dev/null
echo ""

python3 "$SCRIPT_DIR/gate6_report.py" \
    --manifest "$MANIFEST" --out "$OUT_DIR/gate6_report.json" \
    --baseline-utest-log "$OUT_DIR/baseline_utest.log" \
    --baseline-utest-ext-log "$OUT_DIR/baseline_utest_ext.log" \
    --strict-utest-log "$OUT_DIR/strict_utest.log" \
    --strict-utest-ext-log "$OUT_DIR/strict_utest_ext.log" \
    --mixed-utest-log "$OUT_DIR/mixed_utest.log" \
    --mixed-utest-ext-log "$OUT_DIR/mixed_utest_ext.log"

echo ""
echo "[gate6] done -- raw logs + gate6_report.json + gate6_manifest.json in $OUT_DIR"
