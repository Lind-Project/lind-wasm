#!/usr/bin/env bash
# Builds the real OpenBLAS V2 grate from the frozen inference artifact,
# links it against the real, statically-linked libopenblas.a, proves it
# registers cleanly against a real cage, and validates that count
# against a machine-readable manifest -- issue #22's OpenBLAS
# inference-to-runtime integration, Gate 4: "build the real OpenBLAS
# grate".
#
# Deliberately does not check in its own output: the manifest, the
# verified marshal.json, and the grate .c are all reproducible from
# openblas-inference/final/openblas_inference.json + llm-prompts/
# openblas-v6-full + the generator scripts (gate4_manifest.py /
# gen_v2_adapter.py), and must stay that way -- see the plan's own Gate 4
# text. Copy $MANIFEST_OUT out of the work directory before this script
# exits if you need to keep it (e.g. to paste into local-notes); it is
# deleted on exit along with everything else in $WORK.
#
# Usage:
#   tools/marshal-gen/build_openblas_v2_grate.sh [<libopenblas.a path>]
#
# Default archive path matches the sibling lind-wasm-apps checkout
# convention tests/grate-tests/lib-interpose/run_tests.sh's own OPENBLAS_A
# variable uses.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

ARTIFACT="$REPO_ROOT/openblas-inference/final/openblas_inference.json"
PROMPTS_DIR="$REPO_ROOT/llm-prompts/openblas-v6-full"
ARCHIVE="${1:-$(cd "$REPO_ROOT/.." 2>/dev/null && pwd)/lind-wasm-apps/openblas/libopenblas.a}"

if [[ ! -f "$ARTIFACT" ]]; then
    echo "error: frozen artifact not found at $ARTIFACT" >&2
    exit 1
fi
if [[ ! -f "$ARCHIVE" ]]; then
    echo "error: real libopenblas.a not found at $ARCHIVE (build it via lind-wasm-apps/openblas/compile_openblas.sh first, or pass its path as \$1)" >&2
    exit 1
fi

WORK="$(mktemp -d)"
GRATE_SRC="$REPO_ROOT/tests/grate-tests/lib-interpose/libopenblas_v2_grate.c"
cleanup() { rm -rf "$WORK" "$GRATE_SRC" "${GRATE_SRC%.c}.wasm" "${GRATE_SRC%.c}.cwasm"; }
trap cleanup EXIT

MANIFEST="$WORK/gate4_manifest.json"
VERIFIED_MARSHAL="$WORK/openblas_verified.marshal.json"

echo "[build_openblas_v2_grate] step 1/5: building the machine-readable manifest"
echo "[build_openblas_v2_grate]   (import -> archive-verify/complete -> V1/V2 classification, in one pass)"
python3 "$SCRIPT_DIR/gate4_manifest.py" "$ARTIFACT" \
    --prompts-dir "$PROMPTS_DIR" --archive "$ARCHIVE" \
    --out-manifest "$MANIFEST" --out-verified-marshal "$VERIFIED_MARSHAL"
V2_EXPECTED="$(python3 -c "import json; print(json.load(open('$MANIFEST'))['totals']['v2_generated'])")"
echo ""

echo "[build_openblas_v2_grate] step 2/5: generating the self-contained V2 grate from the SAME verified marshal.json"
python3 "$SCRIPT_DIR/gen_v2_adapter.py" "$VERIFIED_MARSHAL" \
    --lib-name libopenblas --out "$GRATE_SRC" \
    --emit-grate --allow-partial
echo ""

echo "[build_openblas_v2_grate] step 3/5: compiling through the Lind toolchain"
"$REPO_ROOT/scripts/lind_compile" -s --compile-grate "$GRATE_SRC" \
    -I "$REPO_ROOT/tests/grate-tests/lib-interpose" "$ARCHIVE"
echo ""

# Exact SET comparison, not just a count: lind_compile's "full" mode
# keeps the intermediate, linked-and-opt'd .wasm on disk right next to
# the final AOT-precompiled .cwasm (confirmed empirically -- the .cwasm
# itself is a Cranelift-compiled native artifact, not something
# wasm-objdump can read at all). Every exported __lind_v2_adapter_<name>
# symbol in THAT .wasm's own export table must be EXACTLY the set
# gen_v2_adapter.py (via gate4_manifest.py's own classification) decided
# to generate -- proving "symbol lists agree with the import report" as
# an actual set-equality check, not merely a count that could agree by
# coincidence (e.g. one symbol silently swapped for another of the same
# total count).
echo "[build_openblas_v2_grate] step 4/5: verifying the exported adapter symbol set exactly matches the manifest"
python3 "$SCRIPT_DIR/verify_exported_symbols.py" "$MANIFEST" "${GRATE_SRC%.c}.wasm"
echo ""

echo "[build_openblas_v2_grate] step 5/5: registration smoke test against a trivial payload"
cat > "$WORK/noop.c" <<'EOF'
int main(void) { return 0; }
EOF
"$REPO_ROOT/scripts/lind_compile" -s "$WORK/noop.c" >/dev/null
mkdir -p "$REPO_ROOT/lindfs/grates"
cp "${GRATE_SRC%.c}.cwasm" "$REPO_ROOT/lindfs/grates/"
cp "$WORK/noop.cwasm" "$REPO_ROOT/lindfs/"
pushd "$REPO_ROOT/lindfs" >/dev/null
SMOKE_OUTPUT="$(timeout 30 "$REPO_ROOT/scripts/lind_run" grates/libopenblas_v2_grate.cwasm /noop.cwasm 2>&1)"
SMOKE_EXIT=$?
popd >/dev/null
rm -f "$REPO_ROOT/lindfs/grates/libopenblas_v2_grate.cwasm" "$REPO_ROOT/lindfs/noop.cwasm"
echo "$SMOKE_OUTPUT"

REGISTERED="$(grep -oP 'registered \K\d+(?=/\d+ handlers)' <<<"$SMOKE_OUTPUT" || true)"
REGISTERED_OF="$(grep -oP 'registered \d+/\K\d+(?= handlers)' <<<"$SMOKE_OUTPUT" || true)"

FAIL=0
if [[ "$SMOKE_EXIT" -ne 0 ]]; then
    echo "[build_openblas_v2_grate] FAIL: smoke test exited $SMOKE_EXIT (expected 0)" >&2
    FAIL=1
fi
if [[ -z "$REGISTERED" || "$REGISTERED" != "$REGISTERED_OF" ]]; then
    echo "[build_openblas_v2_grate] FAIL: could not find a clean 'registered N/N handlers' line" >&2
    FAIL=1
elif [[ "$REGISTERED" != "$V2_EXPECTED" ]]; then
    echo "[build_openblas_v2_grate] FAIL: registered $REGISTERED handlers, manifest expected $V2_EXPECTED V2-generated symbols" >&2
    FAIL=1
fi
if [[ "$FAIL" -ne 0 ]]; then
    exit 1
fi

# Record the validated runtime result INTO the manifest -- this is the
# proof backing Gate 4's own "registration fails closed for any
# generated/runtime ABI disagreement" and "symbol lists agree with the
# import report" acceptance criteria, not merely an assertion in prose.
python3 - "$MANIFEST" "$REGISTERED" "$REGISTERED_OF" <<'PYEOF'
import json, sys
path, registered, registered_of = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
with open(path) as fh:
    manifest = json.load(fh)
manifest["runtime_verification"] = {
    "registered": registered, "attempted": registered_of,
    "matches_v2_generated_count": registered == manifest["totals"]["v2_generated"],
    "exit_code": 0,
}
with open(path, "w") as fh:
    json.dump(manifest, fh, indent=2, sort_keys=True)
    fh.write("\n")
PYEOF

echo ""
echo "[build_openblas_v2_grate] PASS: grate built, compiled, and registered cleanly"
echo "[build_openblas_v2_grate] manifest (copy out before this script exits -- it is deleted on exit):"
cat "$MANIFEST"
