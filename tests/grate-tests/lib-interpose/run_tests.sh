#!/usr/bin/env bash
# Focused lib-interpose grate test suite (NOT full-libc/full-libm, which have
# their own runners under full-libc/ and full-libm/).
#
# Usage:
#   bash tests/grate-tests/lib-interpose/run_tests.sh [--allow-skips]
#
# Every fixture (cage, grate, and the shared libtoy library) is compiled
# fresh from source each run; nothing here trusts a locally-built artifact.
# Each test asserts the exact, ordered lines its cage/grate must print (see
# match_ordered_lines and run_test's header). Failures are categorized (see
# category_for). Every directory with a `*_grate.c` (other than full-libc/
# full-libm) must be declared as a test below or the run fails.
#
# --allow-skips: without it, any skipped maintained test (e.g. a missing
# fixture dependency) fails the run; pass it for an explicitly optional
# local run.
#
# Unsupported wasm value types (v128, funcref/externref) and multi-result
# signatures at lib-3i portal install time (issue #13) can't be reached from
# any C source the Lind toolchain compiles, so that coverage lives in
# src/wasmtime/crates/lib3i-portal-signature-check (raw WAT against the real
# Linker::instance_dylink code path) and is run as a pre-flight step below,
# alongside the raw-arg-slot consistency check.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LINDFS="$REPO_ROOT/lindfs"
GRATES_DIR="$LINDFS/grates"
LIND_COMPILE="$REPO_ROOT/scripts/lind_compile"
LIND_RUN="$REPO_ROOT/scripts/lind_run"

# Route lind_log! diagnostics (Default category is on by default) to stderr,
# where run_test's output capture can see them, instead of their default
# destination (a LIND.log file) -- needed for fail-closed-widesig to assert
# on the rejected-portal diagnostic rather than just a bare exit code.
export LIND_LOG_OUTPUT=stderr

# auto-libz / auto-libz-spike statically link the real libz implementation
# (a static grate must resolve every symbol it interposes at link time) from
# the sibling lind-wasm-apps checkout's zlib build, matching the sibling-repo
# layout convention already used by openblas/compile_openblas.sh. Overridable
# for a non-standard checkout layout; tests that need it SKIP (not FAIL) with
# a clear message if it's missing, rather than failing the whole suite.
LIBZ_A="${LIBZ_A:-$(cd "$REPO_ROOT/.." 2>/dev/null && pwd)/lind-wasm-apps/zlib/libz.a}"

ALLOW_SKIPS="no"
for arg in "$@"; do
    [[ "$arg" == "--allow-skips" ]] && ALLOW_SKIPS="yes"
done

mkdir -p "$GRATES_DIR" "$LINDFS/lib"

# custom-lib/libtoy.c is the preloaded fixture library shared by 5 of the
# tests below (custom-lib, auto-scalar, auto-handle, auto-nested, auto-argv).
# Build it fresh here too, same reasoning as every cage/grate: a stale local
# copy in lindfs/lib/ would silently mask a real regression.
echo "Building shared fixture: libtoy.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/custom-lib/libtoy.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build custom-lib/libtoy.c (needed by 5 tests):"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/custom-lib/libtoy.so" "$LINDFS/lib/libtoy.so"
echo ""

# auto-openblas-daxpy/libblastoy.c is the preloaded fixture library for the
# auto-openblas-daxpy test below (real OpenBLAS symbol names/ABI shapes, see
# that test's own comment for why).
echo "Building shared fixture: libblastoy.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-openblas-daxpy/libblastoy.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-openblas-daxpy/libblastoy.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-openblas-daxpy/libblastoy.so" "$LINDFS/lib/libblastoy.so"
echo ""

# auto-openblas-v2wide/libdaxpby_v2_stub.c is the preloaded fixture library
# for the auto-openblas-v2wide-real test below (its own comment explains why
# a stub is still needed even though the REAL cblas_daxpby is what actually
# answers the interposed call).
echo "Building shared fixture: libdaxpby_v2_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-openblas-v2wide/libdaxpby_v2_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-openblas-v2wide/libdaxpby_v2_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-openblas-v2wide/libdaxpby_v2_stub.so" "$LINDFS/lib/libdaxpby_v2_stub.so"
echo ""

# auto-openblas-v2wide/libdaxpby_fortran_v2_stub.c: same role, for the
# classic Fortran-BLAS daxpby_ form's all-pointer signature.
echo "Building shared fixture: libdaxpby_fortran_v2_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-openblas-v2wide/libdaxpby_fortran_v2_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-openblas-v2wide/libdaxpby_fortran_v2_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-openblas-v2wide/libdaxpby_fortran_v2_stub.so" "$LINDFS/lib/libdaxpby_fortran_v2_stub.so"
echo ""

# auto-conststride/libconststride.c is the preloaded fixture library for the
# auto-conststride test below (constant-sourced StrideVector extent
# operands -- an ordinary contiguous `x[i]` walk with no increment
# argument at all).
echo "Building shared fixture: libconststride.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-conststride/libconststride.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-conststride/libconststride.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-conststride/libconststride.so" "$LINDFS/lib/libconststride.so"
echo ""

# auto-v2wide/libtoy_wide_stub.c is the preloaded fixture library for the
# auto-v2wide tests below. register_lib_handler cannot fabricate a symbol
# out of nothing, only intercept a real one -- this body must never
# actually run once interposed (see fail-registration's identical
# convention); it exists purely to satisfy the dynamic linker.
echo "Building shared fixture: libtoy_wide_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-v2wide/libtoy_wide_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-v2wide/libtoy_wide_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-v2wide/libtoy_wide_stub.so" "$LINDFS/lib/libtoy_wide_stub.so"
echo ""

# auto-v2wide/libtoy_wide_real_stub.c: same role as libtoy_wide_stub.so, but
# matching toy_wide_marshal's REAL direct 9-argument signature (v2wide_cage.c/
# libtoy_wide_stub.c instead pack those into one struct pointer, for
# v2wide_shim_grate.c's V1-shim workaround -- see that grate's own doc for
# why the shim exists at all).
echo "Building shared fixture: libtoy_wide_real_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-v2wide/libtoy_wide_real_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-v2wide/libtoy_wide_real_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-v2wide/libtoy_wide_real_stub.so" "$LINDFS/lib/libtoy_wide_real_stub.so"
echo ""

# auto-v2wide/libtoy_handle_v2_stub.c: preloaded fallback for the V2 handle
# round-trip proof -- same fail-closed-stub role as
# libtoy_wide_real_stub.so above.
echo "Building shared fixture: libtoy_handle_v2_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-v2wide/libtoy_handle_v2_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-v2wide/libtoy_handle_v2_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-v2wide/libtoy_handle_v2_stub.so" "$LINDFS/lib/libtoy_handle_v2_stub.so"
echo ""

# fail-closed/libextentexpr_stub.c: preloaded fallback for the general
# lind_extent_expr tree tests (issue #22 / OpenBLAS inference integration,
# Gate 1) -- same fail-closed-stub role as the others above.
echo "Building shared fixture: libextentexpr_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/fail-closed/libextentexpr_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build fail-closed/libextentexpr_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/fail-closed/libextentexpr_stub.so" "$LINDFS/lib/libextentexpr_stub.so"
echo ""

# auto-v2wide/v2wide_adapters.c is generated fresh here with
# tools/marshal-gen/gen_v2_adapter.py from the checked-in
# auto-v2wide/v2wide.spec.json) -- same "compiled fresh from source each
# run" reasoning as every other fixture above, one level up: this is what
# proves the generator's CURRENT output builds and behaves correctly, not a
# frozen snapshot that could silently go stale under it.
echo "Generating auto-v2wide/v2wide_adapters.c from v2wide.spec.json"
if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_v2_adapter.py" \
        "$SCRIPT_DIR/auto-v2wide/v2wide.spec.json" \
        --lib-name libtoy_wide --manifest-version 2 \
        --out "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c" \
        > /tmp/lib-interpose-gen-v2wide.log 2>&1; then
    echo "FATAL: gen_v2_adapter.py failed to generate the v2wide adapter:" >&2
    cat /tmp/lib-interpose-gen-v2wide.log >&2
    exit 1
fi
if ! grep -q "__lind_v2_adapter_toy_wide_marshal" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c"; then
    echo "FATAL: generated v2wide adapter is missing __lind_v2_adapter_toy_wide_marshal" >&2
    exit 1
fi
echo ""

PASS=0
FAIL=0
SKIP=0
declare -A CATEGORY_COUNTS
declare -a FAILURES
declare -a DECLARED_TESTS

pass_test() {
    echo "  PASS  $1"
    PASS=$((PASS + 1))
}

fail_test() {
    local name="$1" category="$2" output="$3"
    echo "  FAIL  $name [$category]"
    echo "$output" | tail -12 | sed 's/^/         /'
    FAIL=$((FAIL + 1))
    CATEGORY_COUNTS["$category"]=$(( ${CATEGORY_COUNTS["$category"]:-0} + 1 ))
    FAILURES+=("$name [$category]")
}

skip_test() {
    echo "  SKIP  $1 -- $2"
    SKIP=$((SKIP + 1))
}

# compile_src <src.c> [extra clang args...]
# Compiles a dynamic (non-grate) executable next to its source. -fno-builtin
# disables builtin lowering, so a call like memcpy/strlen/memcmp stays a real
# interposable call/import instead of being inlined or constant-folded away.
compile_src() {
    local src="$1"; shift
    "$LIND_COMPILE" "$src" -- -fno-builtin "$@" > /tmp/lib-interpose-compile.log 2>&1
}

# compile_grate <src.c> [extra clang/source args...]
compile_grate() {
    local src="$1"; shift
    "$LIND_COMPILE" -s --compile-grate --fpcast-emu "$src" -I "$SCRIPT_DIR" "$@" \
        > /tmp/lib-interpose-compile.log 2>&1
}

# run_test <name> <cage_src|""> <grate_src> <preload_csv|""> <strict:yes|no> \
#          <run_args...> -- <expect...> -- <evidence...> [-- <forbidden...>]
#
# cage_src/grate_src: source path relative to this directory; output name is
#   derived automatically (lind_compile always names <src>.c -> <src>.cwasm,
#   next to the source). Pass "" for cage_src if there's no cage of its own
#   (e.g. zlib-python, whose "cage" is a pre-existing lindfs-resident app).
#
# preload_csv: comma-separated `module=path` preload specs (e.g.
#   "env=/lib/libtoy.so" or "env=/lib/libz.so,env=/lib/libpython3.14.so").
# strict: if "yes", every preload above gets `:interposed` appended -- only
#   safe for a preload whose library surface the cage exercises ENTIRELY
#   through registered handlers (see each test's own note below for why).
# expect: exact literal lines that MUST all appear in the combined output,
#   in this order -- catches "prints PASS but something afterward is wrong"
#   and "prints PASS by coincidence without dispatch ever firing" alike.
# evidence: same as expect, but specifically the marker line(s) that could
#   ONLY appear if the grate's handler genuinely ran (as opposed to a silent
#   fallback to the real, uninterposed implementation happening to produce
#   the same result) -- required in addition to `expect` for any test whose
#   handler's return value alone is indistinguishable from what the real,
#   uninterposed function would have produced.
# forbidden: optional (the trailing `--` may be omitted entirely if empty)
#   -- lines that must NOT appear anywhere in the output. A fail-closed
#   rejection test must forbid its handler's own "handler ran" marker: a
#   marshaller that wrongly invokes the handler, which then dereferences a
#   foreign pointer and traps, produces the same caller-visible GRATE_ERR as
#   a correct rejection -- expect[] alone can't tell those apart.
run_test() {
    local name="$1" cage_src="$2" grate_src="$3" preload_csv="$4" strict="$5"
    shift 5
    local run_args=()
    while [[ "$#" -gt 0 && "$1" != "--" ]]; do run_args+=("$1"); shift; done
    shift # consume the first --
    local expect=()
    while [[ "$#" -gt 0 && "$1" != "--" ]]; do expect+=("$1"); shift; done
    shift # consume the second --
    local evidence=()
    while [[ "$#" -gt 0 && "$1" != "--" ]]; do evidence+=("$1"); shift; done
    local forbidden=()
    if [[ "$#" -gt 0 ]]; then
        shift # consume the third --
        forbidden=("$@")
    fi

    DECLARED_TESTS+=("$name")

    # lind_compile always names its output after the (first) source file,
    # next to it: foo.c -> foo.cwasm.
    local grate_cwasm="$(basename "${grate_src%.c}").cwasm"

    # -- build --
    if [[ -n "$cage_src" ]]; then
        if ! compile_src "$SCRIPT_DIR/$cage_src"; then
            fail_test "$name" build "COMPILE_STEP_FAILED (cage)
$(cat /tmp/lib-interpose-compile.log)"
            return
        fi
    fi
    if ! compile_grate "$SCRIPT_DIR/$grate_src" "${GRATE_EXTRA[@]}"; then
        fail_test "$name" build "COMPILE_STEP_FAILED (grate)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi

    # -- stage --
    local staged=()
    cp "$SCRIPT_DIR/$(dirname "$grate_src")/$grate_cwasm" "$GRATES_DIR/"
    staged+=("$GRATES_DIR/$grate_cwasm")
    if [[ -n "$cage_src" ]]; then
        local cage_cwasm="$(basename "${cage_src%.c}").cwasm"
        local cage_dst="$LINDFS/$cage_cwasm"
        cp "$SCRIPT_DIR/$(dirname "$cage_src")/$cage_cwasm" "$cage_dst"
        staged+=("$cage_dst")
    fi

    # -- assemble preload args --
    local preload_args=()
    if [[ -n "$preload_csv" ]]; then
        local IFS=','
        local p
        for p in $preload_csv; do
            if [[ "$strict" == "yes" ]]; then
                preload_args+=(--preload "${p}:interposed")
            else
                preload_args+=(--preload "$p")
            fi
        done
    fi

    # -- run, correctly capturing the ACTUAL command's exit code (not `true`'s) --
    local output exit_code
    output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" "${preload_args[@]}" \
        "grates/$grate_cwasm" "${run_args[@]}" 2>&1)
    exit_code=$?

    rm -f "${staged[@]}"

    # -- validate: expect[] and evidence[] are each their own ordered
    # sequence of complete lines, independent of each other (evidence
    # commonly precedes expect in the real output). --
    local missing=() m line
    if ! m="$(match_ordered_lines "$output" "${expect[@]}")"; then
        while IFS= read -r line; do missing+=("$line"); done <<<"$m"
    fi
    if [[ ${#evidence[@]} -gt 0 ]] && ! m="$(match_ordered_lines "$output" "${evidence[@]}")"; then
        while IFS= read -r line; do missing+=("[dispatch evidence] $line"); done <<<"$m"
    fi
    if [[ ${#forbidden[@]} -gt 0 ]] && ! m="$(find_forbidden_lines "$output" "${forbidden[@]}")"; then
        while IFS= read -r line; do missing+=("[forbidden, but present] $line"); done <<<"$m"
    fi

    if [[ "$(decide_outcome "$exit_code" "${#missing[@]}")" == "fail" ]]; then
        if [[ "$exit_code" -ne 0 ]]; then
            fail_test "$name" "$(category_for "$output" "$exit_code")" "$output"
        else
            fail_test "$name" "$(category_for "$output" 0)" \
                "validation failed:
$(printf '  - %s\n' "${missing[@]}")
--- actual output ---
$output"
        fi
    else
        pass_test "$name"
    fi
}

# Pre-flight: the interposition transport's raw-ABI-slot capacity is
# duplicated (not shared) across inference/generation/runtime -- fail fast,
# before compiling anything, if those copies have drifted out of sync.
if ! bash "$SCRIPT_DIR/check_raw_arg_slot_consistency.sh"; then
    echo "FATAL: raw-arg-slot constant consistency check failed (see above)" >&2
    exit 1
fi
echo ""

# Pre-flight: the general lind_extent_expr tree's depth/node-count
# ceilings are likewise duplicated across generation and runtime -- same
# reasoning as the raw-arg-slot check just above.
if ! bash "$SCRIPT_DIR/check_extent_expr_bounds_consistency.sh"; then
    echo "FATAL: extent-expr bounds consistency check failed (see above)" >&2
    exit 1
fi
echo ""

# Pre-flight: unsupported wasm value types (v128, funcref/externref) and
# multi-result signatures at lib-3i portal install time (issue #13). No C
# source compiles to those shapes, so this is a separate Rust crate built
# on raw WAT -- see its own doc comment for why. Built fresh each run, same
# reasoning as every cage/grate/fixture above: a stale local binary would
# silently mask a real regression in linker.rs's portal-install validation.
echo "Running lib3i-portal-signature-check (issue #13)..."
if ! (cd "$REPO_ROOT/src/wasmtime" && cargo run --quiet -p lib3i-portal-signature-check); then
    echo "FATAL: lib3i-portal-signature-check failed (see above)" >&2
    exit 1
fi
echo ""

# Pre-flight: no config/profile knob can weaken lind_marshal.h's own runtime
# checks (issue #26/#27 follow-up, item 8) -- confidence/policy/heuristic/
# contract are purely inference-time bookkeeping (see CONFIG.md) that never
# reaches the runtime spec gen_grate.py emits; this pins that by construction
# instead of trusting it to stay true. A hit here means something started
# threading config-derived trust into a runtime check, which must not happen.
echo "Checking lind_marshal.h has no config/confidence-conditional runtime check..."
if grep -Eqi '\b(confidence|policy|relaxed|heuristics?|contracts)\b' "$SCRIPT_DIR/lind_marshal.h"; then
    echo "FATAL: lind_marshal.h references a config/confidence concept -- a profile" >&2
    echo "must never be able to weaken overflow/pointer-provenance/arena/ABI-width/" >&2
    echo "copy-back validation (issue #26/#27 item 8)" >&2
    exit 1
fi
echo ""

# Pre-flight: OpenBLAS coverage/required-symbol gate and, from it, the
# auto-openblas-daxpy grate itself (issue #26/#27 follow-up). Both need a
# real openblas.marshal.json (tools/marshal-infer/infer_openblas.sh, which
# needs a sibling lind-wasm-apps/openblas checkout) -- gitignored, so this
# skips (not fails) the whole run when it's absent, same as the LIBZ_A check.
OPENBLAS_JSON="$REPO_ROOT/openblas.marshal.json"
OPENBLAS_DAXPY_GEN_OK="no"
if [[ -f "$OPENBLAS_JSON" ]]; then
    echo "Running OpenBLAS coverage/required-symbol gate..."
    if ! python3 "$REPO_ROOT/tools/marshal-infer/openblas_coverage.py" "$OPENBLAS_JSON"; then
        echo "FATAL: OpenBLAS coverage/required-symbol gate failed (see above)" >&2
        exit 1
    fi
    echo ""

    # Regenerates straight from the LIVE openblas.marshal.json every run --
    # same "nothing here trusts a locally-built artifact" reasoning as every
    # cage/grate above, one level up: this is what actually proves TODAY's
    # inference+config output produces a runnable handler for these two
    # required symbols (item 6), not a frozen fixture that could go stale
    # the moment inference's output shape changes under it.
    echo "Generating auto-openblas-daxpy/openblas_daxpy_auto_grate.c from $(basename "$OPENBLAS_JSON")"
    if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_grate.py" "$OPENBLAS_JSON" \
            --lib-name libblastoy --only cblas_daxpy,daxpy_ \
            --out "$SCRIPT_DIR/auto-openblas-daxpy/openblas_daxpy_auto_grate.c" \
            > /tmp/lib-interpose-gen-openblas.log 2>&1; then
        echo "FATAL: gen_grate.py failed to generate cblas_daxpy/daxpy_ handlers:" >&2
        cat /tmp/lib-interpose-gen-openblas.log >&2
        exit 1
    fi
    gen_missing=()
    for sym in cblas_daxpy daxpy_; do
        grep -q "\"$sym\"" "$SCRIPT_DIR/auto-openblas-daxpy/openblas_daxpy_auto_grate.c" \
            || gen_missing+=("$sym")
    done
    if [[ ${#gen_missing[@]} -gt 0 ]]; then
        echo "FATAL: generated grate is missing required symbol(s): ${gen_missing[*]}" >&2
        exit 1
    fi
    OPENBLAS_DAXPY_GEN_OK="yes"
    echo ""
else
    echo "Skipping OpenBLAS coverage gate and auto-openblas-daxpy generation:" \
         "$OPENBLAS_JSON not found (run tools/marshal-infer/infer_openblas.sh first)"
    echo ""
fi

# Pre-flight for the real-library wide-call proof: needs the same live
# openblas.marshal.json as auto-openblas-daxpy
# above, PLUS a real, already-built wasm32 libopenblas.a (produced by
# lind-wasm-apps/openblas/compile_openblas.sh, the same archive
# infer_openblas.sh itself analyzes) to link the REAL implementation into
# the grate -- not a hand-written stand-in the way libblastoy.c is for V1.
# Both are gitignored/external, so this skips (not fails) when either is
# absent, same posture as the OPENBLAS_JSON check above.
APPS_ROOT="${LIND_WASM_APPS_ROOT:-$(cd "$REPO_ROOT/.." && pwd)/lind-wasm-apps}"
OPENBLAS_A="$APPS_ROOT/openblas/libopenblas.a"
OPENBLAS_V2_GEN_OK="no"
if [[ "$OPENBLAS_DAXPY_GEN_OK" == "yes" && -f "$OPENBLAS_A" ]]; then
    # Regenerated straight from the LIVE openblas.marshal.json every run,
    # same "nothing here trusts a locally-built artifact" reasoning as
    # openblas_daxpy_auto_grate.c above -- this proves TODAY's inference
    # output (specifically, Infer.cpp's annotateWideRawArgSlots no longer
    # force_localing cblas_daxpby purely for its 7-raw-ABI-slot width) is
    # what gen_v2_adapter.py actually consumes, not a frozen snapshot.
    # Both the CBLAS (by-value, LIND_EXTENT_VALUE) and classic Fortran-BLAS
    # (by-reference, LIND_EXTENT_POINTEE_I32) forms of daxpby land in ONE
    # generated file -- proving both StrideVector extent-source conventions
    # survive the real V2 path, not just one of them.
    echo "Generating auto-openblas-v2wide/daxpby_v2_adapter.c from $(basename "$OPENBLAS_JSON")"
    if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_v2_adapter.py" "$OPENBLAS_JSON" \
            --lib-name openblas --only cblas_daxpby,daxpby_ --manifest-version 1 \
            --out "$SCRIPT_DIR/auto-openblas-v2wide/daxpby_v2_adapter.c" \
            > /tmp/lib-interpose-gen-openblas-v2.log 2>&1; then
        echo "FATAL: gen_v2_adapter.py failed to generate the daxpby V2 adapters:" >&2
        cat /tmp/lib-interpose-gen-openblas-v2.log >&2
        exit 1
    fi
    gen_v2_missing=()
    for sym in __lind_v2_adapter_cblas_daxpby __lind_v2_adapter_daxpby_; do
        grep -q "$sym" "$SCRIPT_DIR/auto-openblas-v2wide/daxpby_v2_adapter.c" || gen_v2_missing+=("$sym")
    done
    if [[ ${#gen_v2_missing[@]} -gt 0 ]]; then
        echo "FATAL: generated V2 adapter is missing: ${gen_v2_missing[*]}" >&2
        exit 1
    fi
    OPENBLAS_V2_GEN_OK="yes"
    echo ""
else
    echo "Skipping auto-openblas-v2wide-real generation:" \
         "needs both $OPENBLAS_JSON and $OPENBLAS_A" \
         "(run tools/marshal-infer/infer_openblas.sh and" \
         "lind-wasm-apps/openblas/compile_openblas.sh first)"
    echo ""
fi

# Regenerates straight from a fresh inference run over libconststride.c
# every run -- same "nothing here trusts a locally-built artifact"
# reasoning as auto-openblas-daxpy above, one level up: this proves TODAY's
# inference+generation output produces a runnable handler for a
# constant-sourced StrideVector extent operand, not a frozen fixture that
# could go stale the moment that output shape changes under it. No
# gitignored external checkout needed (unlike OpenBLAS), so this always runs.
echo "Generating auto-conststride/conststride_auto_grate.c from libconststride.c"
if ! ( cd "$SCRIPT_DIR/auto-conststride" && "$LIND_COMPILE" --emit-marshal libconststride.c ) \
        > /tmp/lib-interpose-gen-conststride.log 2>&1; then
    echo "FATAL: lind_compile --emit-marshal failed for libconststride.c:" >&2
    cat /tmp/lib-interpose-gen-conststride.log >&2
    exit 1
fi
if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_grate.py" \
        "$SCRIPT_DIR/auto-conststride/libconststride.marshal.json" \
        --lib-name libconststride \
        --out "$SCRIPT_DIR/auto-conststride/conststride_auto_grate.c" \
        > /tmp/lib-interpose-gen-conststride2.log 2>&1; then
    echo "FATAL: gen_grate.py failed to generate toy_vec_scale handler:" >&2
    cat /tmp/lib-interpose-gen-conststride2.log >&2
    exit 1
fi
if ! grep -q '"toy_vec_scale"' "$SCRIPT_DIR/auto-conststride/conststride_auto_grate.c"; then
    echo "FATAL: generated grate is missing toy_vec_scale" >&2
    exit 1
fi
if ! grep -q 'LIND_EXTENT_CONSTANT' "$SCRIPT_DIR/auto-conststride/conststride_auto_grate.c"; then
    echo "FATAL: generated grate's toy_vec_scale handler does not use a" \
         "constant-sourced extent operand -- inference regressed" >&2
    exit 1
fi
echo ""

# V2 sibling of the generation above, from the SAME freshly-generated
# libconststride.marshal.json: gen_v2_adapter.py --emit-grate is the
# self-contained-grate generator all new library-interposition work should
# use (issue #22); V1's gen_grate.py above stays available for
# existing/legacy usage, unchanged.
echo "Generating auto-conststride/conststride_v2_grate.c from libconststride.marshal.json"
if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_v2_adapter.py" \
        "$SCRIPT_DIR/auto-conststride/libconststride.marshal.json" \
        --lib-name libconststride --emit-grate --manifest-version 1 \
        --out "$SCRIPT_DIR/auto-conststride/conststride_v2_grate.c" \
        > /tmp/lib-interpose-gen-conststride-v2.log 2>&1; then
    echo "FATAL: gen_v2_adapter.py --emit-grate failed to generate toy_vec_scale:" >&2
    cat /tmp/lib-interpose-gen-conststride-v2.log >&2
    exit 1
fi
if ! grep -q '__lind_v2_adapter_toy_vec_scale' "$SCRIPT_DIR/auto-conststride/conststride_v2_grate.c"; then
    echo "FATAL: generated V2 grate is missing __lind_v2_adapter_toy_vec_scale" >&2
    exit 1
fi
if ! grep -q 'register_lib_handler_v2' "$SCRIPT_DIR/auto-conststride/conststride_v2_grate.c"; then
    echo "FATAL: generated V2 grate does not register through register_lib_handler_v2" >&2
    exit 1
fi
echo ""

# Self-contained V2 grate for the handle round-trip proof, generated the
# same way from a hand-written spec (this is a
# synthetic fixture, like auto-v2wide/v2wide.spec.json, not real inferred
# code): LIND_RET_HANDLE/LIND_ARG_HANDLE through the fully generated V2
# pipeline, not a hand-written grate.
echo "Generating auto-v2wide/handle_v2_grate.c from handle_v2.spec.json"
if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_v2_adapter.py" \
        "$SCRIPT_DIR/auto-v2wide/handle_v2.spec.json" \
        --lib-name handle_v2 --emit-grate --manifest-version 1 \
        --out "$SCRIPT_DIR/auto-v2wide/handle_v2_grate.c" \
        > /tmp/lib-interpose-gen-handle-v2.log 2>&1; then
    echo "FATAL: gen_v2_adapter.py --emit-grate failed to generate handle_v2:" >&2
    cat /tmp/lib-interpose-gen-handle-v2.log >&2
    exit 1
fi
if ! grep -q '__lind_v2_adapter_toy_ctx_create_v2' "$SCRIPT_DIR/auto-v2wide/handle_v2_grate.c"; then
    echo "FATAL: generated handle_v2 grate is missing __lind_v2_adapter_toy_ctx_create_v2" >&2
    exit 1
fi
echo ""

echo "=== lib-interpose focused test suite ==="
echo ""

# --------------------------------------------------------------------------
# libc-rand: intercepts rand() and returns a fixed value. Not strict-safe:
# the cage also uses printf/assert from libc, which have no handlers.
# --------------------------------------------------------------------------
GRATE_EXTRA=()
run_test "libc-rand" \
    "libc-rand/libc-rand.c" \
    "libc-rand/libc-rand_grate.c" \
    "" "no" \
    "/libc-rand.cwasm" \
    -- "[Cage] rand() = 42" "[Cage] PASS" "[Grate|libc-rand] PASS" \
    -- # 42 three times in a row cannot come from the real rand()

# --------------------------------------------------------------------------
# libc-strlen: intercepts strlen() and returns len*2.
# --------------------------------------------------------------------------
GRATE_EXTRA=()
run_test "libc-strlen" \
    "libc-strlen/libc-strlen.c" \
    "libc-strlen/libc-strlen_grate.c" \
    "" "no" \
    "/libc-strlen.cwasm" \
    -- "[Cage] strlen(\"hello\") = 10" "[Cage] PASS" "[Grate|libc-strlen] PASS" \
    -- # real strlen("hello")=5 != 10

# --------------------------------------------------------------------------
# custom-lib: intercepts toy_add/toy_mul from a preloaded wasm library.
# Strict-safe: the cage calls nothing else from libtoy.
# --------------------------------------------------------------------------
GRATE_EXTRA=()
run_test "custom-lib" \
    "custom-lib/custom-lib.c" \
    "custom-lib/custom-lib_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/custom-lib.cwasm" \
    -- "[Cage] toy_add(3, 4) = 14" "[Cage] toy_mul(5, 6) = 11" "[Cage] PASS" "[Grate|lib-interpose] PASS" \
    --

# --------------------------------------------------------------------------
# zlib-python: intercepts deflate() so Python's zlib.compress() returns
# b"LIND". No cage source of its own (the Python interpreter + test-zlib.py
# are lindfs-resident, out of this directory's scope). Not strict-safe: the
# cage exercises far more of libz/libpython than these 3 symbols.
# --------------------------------------------------------------------------
GRATE_EXTRA=()
run_test "zlib-python" \
    "" \
    "zlib-python/zlib-python_grate.c" \
    "env=/lib/libz.so,env=/lib/libpython3.14.so" "no" \
    "/usr/local/bin/python" "/test-zlib.py" \
    -- "Compressed bytes: b'LIND'" "[Grate|zlib-python] PASS: deflate intercepted 1 time(s), Python exited 0" \
    -- "[Grate|zlib-python] deflate intercepted — wrote 4 fixed bytes, returning Z_STREAM_END"

# --------------------------------------------------------------------------
# Stage-1 automated marshalling tests
# --------------------------------------------------------------------------

# auto-scalar: SCALAR spec; handler returns a*b instead of the real a+b.
GRATE_EXTRA=()
run_test "auto-scalar" \
    "auto-scalar/auto-scalar.c" \
    "auto-scalar/auto-scalar_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/auto-scalar.cwasm" \
    -- "[Cage|auto-scalar] PASS: toy_add(10,3) = 30 (intercepted as multiply)" \
    --

# auto-memcpy: PTR IN/OUT + return alias. The handler calls the REAL memcpy,
# so its result is indistinguishable from an uninterposed call on its own --
# dispatch evidence (the grate's own trace line) is required in addition.
GRATE_EXTRA=()
run_test "auto-memcpy" \
    "auto-memcpy/auto-memcpy.c" \
    "auto-memcpy/auto-memcpy_grate.c" \
    "" "no" \
    "/auto-memcpy.cwasm" \
    -- "[Cage|auto-memcpy] PASS: memcpy copied \"hello, lind!\", return == dst" \
    -- "[Grate|auto-memcpy] memcpy intercepted: n=13"

# auto-strncpy: same pattern as auto-memcpy (real strncpy call, needs
# dispatch evidence).
GRATE_EXTRA=()
run_test "auto-strncpy" \
    "auto-strncpy/auto-strncpy.c" \
    "auto-strncpy/auto-strncpy_grate.c" \
    "" "no" \
    "/auto-strncpy.cwasm" \
    -- "[Cage|auto-strncpy] PASS: strncpy produced \"lind-wasm\", return == dst" \
    -- "[Grate|auto-strncpy] strncpy intercepted: n=32"

# auto-strncpy-short: regression guard for a known marshalling-safety gap
# (see auto-strncpy.c) -- asserts the current, deterministic (non-crashing
# but incorrect) behavior so any change to it is caught. Reuses the
# already-built auto-strncpy grate.
GRATE_EXTRA=()
run_test "auto-strncpy-short" \
    "auto-strncpy/auto-strncpy-short.c" \
    "auto-strncpy/auto-strncpy_grate.c" \
    "" "no" \
    "/auto-strncpy-short.cwasm" \
    -- "[Cage|auto-strncpy-short] dst=\"\"" "[Grate|auto-strncpy] PASS" \
    --

# --------------------------------------------------------------------------
# Stage-3 marshalling tests
# --------------------------------------------------------------------------

# auto-cstr: LIND_SIZE_CSTR; intercepts strlen, returns len*2.
GRATE_EXTRA=()
run_test "auto-cstr" \
    "auto-cstr/auto-cstr.c" \
    "auto-cstr/auto-cstr_grate.c" \
    "" "no" \
    "/auto-cstr.cwasm" \
    -- "[Cage|auto-cstr] PASS: strlen(\"hello\") = 10 (intercepted as len*2)" \
    --

# auto-compress2: LIND_SIZE_FROM_ARG_POINTEE; the handler ignores the real
# source data and writes a fixed "LIND"/destLen=4, so no real zlib output
# could coincidentally match -- no extra dispatch evidence needed.
# Strict-safe: the cage calls only compress2 from libz.
GRATE_EXTRA=()
run_test "auto-compress2" \
    "auto-compress2/auto-compress2.c" \
    "auto-compress2/auto-compress2_grate.c" \
    "env=/lib/libz.so" "yes" \
    "/auto-compress2.cwasm" \
    -- "[Cage|auto-compress2] PASS: got \"LIND\" destLen=4" \
    --

# auto-memchr: LIND_RET_PTR_INTO_ARG; the handler calls the real memchr and
# the *value* found (offset 2) is by design identical to what an
# uninterposed call would report -- this test is verifying the shadow-to-
# source-cage pointer translation, not an output difference, so the grate's
# own dispatch trace is the only possible evidence and is required.
GRATE_EXTRA=()
run_test "auto-memchr" \
    "auto-memchr/auto-memchr.c" \
    "auto-memchr/auto-memchr_grate.c" \
    "" "no" \
    "/auto-memchr.cwasm" \
    -- "[Cage|auto-memchr] PASS: found 'l' at offset 2" \
    -- "[Grate|auto-memchr] memchr dispatched, offset=2"

# auto-handle: LIND_ARG_HANDLE + LIND_RET_HANDLE. The round-tripped value
# (42) is unavoidably identical either way by design (it's a correctness
# test of the handle table, not a wrong-value marker) -- the grate's own
# create/get/close trace lines are the only possible evidence and are
# required. Strict-safe: the cage calls only the 3 registered libtoy symbols.
GRATE_EXTRA=()
run_test "auto-handle" \
    "auto-handle/auto-handle.c" \
    "auto-handle/auto-handle_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/auto-handle.cwasm" \
    -- "[Cage|auto-handle] PASS: create/get_val/close round-trip, val=42" \
    -- "[Grate|auto-handle] toy_ctx_create dispatched, val=42" \
       "[Grate|auto-handle] toy_ctx_get_val dispatched, val=42" \
       "[Grate|auto-handle] toy_ctx_close dispatched"

# auto-nested: nested struct layout; handler returns sum+1 instead of the
# real sum. Strict-safe: the cage calls only toy_buf_checksum from libtoy.
GRATE_EXTRA=()
run_test "auto-nested" \
    "auto-nested/auto-nested.c" \
    "auto-nested/auto-nested_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/auto-nested.cwasm" \
    -- "[Cage|auto-nested] PASS: toy_buf_checksum = 199 (sum+1)" \
    --

# --------------------------------------------------------------------------
# Auto-generated (gen_grate.py-style) marshalling tests: a single generic
# ctx-dispatch pass_fptr_to_wt calling the REAL underlying function for
# every registered symbol -- every one of these needs dispatch evidence
# (the grate's own registration/trace lines), since the checked values are
# all real, correct results a completely uninterposed run would print too.
# --------------------------------------------------------------------------

# auto-argv: LIND_SIZE_PTR_ARRAY (NULL-terminated argv marshalling).
# Strict-safe: the cage calls only toy_argv_len from libtoy.
GRATE_EXTRA=("$SCRIPT_DIR/auto-argv/toy_argv_impl.c")
run_test "auto-argv" \
    "auto-argv/argv_app.c" \
    "auto-argv/argv_auto_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/argv_app.cwasm" \
    -- "[argv-app] PASS: toy_argv_len(argv) = 11 (ptr_array marshalled)" \
    -- "[libtoy-grate] registered 1/1 handlers"

# auto-libz: broad libz surface (adler32/crc32/compress2/uncompress +
# force_local zlibVersion). NOT strict-safe: zlibVersion is deliberately
# un-registered and would trap under :interposed. Needs the sibling repo's
# static libz.a (see LIBZ_A above) -- skips gracefully if unavailable.
if [[ -f "$LIBZ_A" ]]; then
    GRATE_EXTRA=("$LIBZ_A")
    run_test "auto-libz" \
        "auto-libz/libz_app.c" \
        "auto-libz/libz_auto_grate.c" \
        "env=/lib/libz.so" "no" \
        "/libz_app.cwasm" \
        -- "  PASS  adler32  = 0x11e60398 (want 0x11e60398)" \
           "  PASS  crc32  = 0xcbf43926 (want 0xcbf43926)" \
           "  PASS  compress  rc=0, valid size" \
           "  PASS  uncompress  ru=0 back_len=90 (want 90, roundtrip)" \
           "[libz-app] 6 passed, 0 failed (of marshalled+local libz calls)" \
        -- "[libz-grate] registered 48/48 handlers"
else
    DECLARED_TESTS+=("auto-libz")
    skip_test "auto-libz" "libz.a not found at $LIBZ_A (needs a sibling lind-wasm-apps checkout with zlib built)"
fi

# auto-libz-spike: single-symbol adler32 mechanism-verification spike
# (documented as intentionally calling the real adler32). Strict-safe
# (single symbol). Also needs libz.a.
if [[ -f "$LIBZ_A" ]]; then
    GRATE_EXTRA=("$LIBZ_A")
    run_test "auto-libz-spike" \
        "auto-libz-spike/spike_cage.c" \
        "auto-libz-spike/spike_grate.c" \
        "env=/lib/libz.so" "yes" \
        "/spike_cage.cwasm" \
        -- "[Cage|spike] PASS: adler32(\"Wikipedia\")=0x11e60398" \
        -- "[Grate|spike] dispatch returned 0x11e60398"
else
    DECLARED_TESTS+=("auto-libz-spike")
    skip_test "auto-libz-spike" "libz.a not found at $LIBZ_A (needs a sibling lind-wasm-apps checkout with zlib built)"
fi

# auto-libc: 8-symbol libc string-function surface (strlen/strnlen/memcmp/
# strcmp/strncmp/memchr/strchr/strtol), exercising out_ptr_into_arg1
# (strtol's endptr) and ptr_into_arg (strchr/memchr) on top of the simpler
# specs the other tests cover. No extra --preload: the wrapper's built-in
# libc.cwasm preload already provides these symbols.
GRATE_EXTRA=()
run_test "auto-libc" \
    "auto-libc/libc_app.c" \
    "auto-libc/libc_auto_grate.c" \
    "" "no" \
    "/libc_app.cwasm" \
    -- "  PASS  strlen  = 11 (want 11)" \
       "  PASS  strnlen  = 5 (want 5)" \
       "  PASS  memcmp  eq=0 lt=-1" \
       "  PASS  strcmp  eq=0 lt=-1" \
       "  PASS  strncmp  = 0 (want 0)" \
       "  PASS  memchr  off=6 (want 6, ptr-into-arg)" \
       "  PASS  strchr  off=4 (want 4, ptr-into-arg)" \
       "  PASS  strtol(endptr)  v=12345 endoff=5 (want 12345, 5)" \
       "[libc-app] 8 passed, 0 failed" \
    -- "[libc-grate] registered 8/8 handlers"

# --------------------------------------------------------------------------
# fail-closed: lind_marshal.h's fail-closed paths (issue #6). Each mode
# below deliberately triggers one marshalling failure against a REAL
# interposed function (memcpy/strlen from libc, toy_ctx_get_val/
# toy_buf_checksum/toy_argv_len from libtoy) and expects the call to be
# rejected -- the grate's own handler must never run, and the caller sees
# threei::GRATE_ERR rather than a real (or foreign-pointer-derived) result.
# "ok" is the control: the same shapes with valid arguments, proving these
# are genuine rejections and not a broken/always-failing grate.
# The libtoy preload is marked :interposed (strict mode): the cage touches
# only the 3 registered libtoy symbols, so this also directly demonstrates
# criterion #7 (rejection under strict mode, not a silent local/mixed-mode
# fallback).
#
# Each rejection mode also FORBIDS its handler's own "handler ran" marker.
# Without that, a marshaller bug that wrongly invokes the handler -- which
# then dereferences the foreign/corrupt data itself and traps -- would
# produce the exact same caller-visible GRATE_ERR as a correct rejection,
# passing the test despite the isolation violation actually happening.
# --------------------------------------------------------------------------
for mode_desc in \
    "arena:memcpy" \
    "overflow:memcpy" \
    "badcopy:memcpy" \
    "unsized:strlen" \
    "handle:toy_ctx_get_val" \
    "nested:toy_buf_checksum" \
    "argv:toy_argv_len"
do
    mode="${mode_desc%%:*}"
    fn="${mode_desc#*:}"
    GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
    run_test "fail-closed-$mode" \
        "fail-closed/failclosed_cage.c" \
        "fail-closed/failclosed_grate.c" \
        "env=/lib/libtoy.so" "yes" \
        "/failclosed_cage.cwasm" "$mode" \
        -- "[Cage|fail-closed] PASS: $mode rejected (GRATE_ERR)" \
        -- \
        -- "[Grate|fail-closed] $fn handler ran (should not happen)"
done

# Control: the same shapes with valid arguments, proving the rejections
# above are genuine and not a broken/always-failing grate. Here the
# handler running is the expected/desired behavior, not forbidden.
GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-ok" \
    "fail-closed/failclosed_cage.c" \
    "fail-closed/failclosed_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/failclosed_cage.cwasm" "ok" \
    -- "[Cage|fail-closed] PASS: ok completed normally" \
    -- "[Grate|fail-closed] memcpy handler ran (should not happen)"

# Additional adversarial cases: bounds checks on spec-provided indices
# (badspec_grate.c registers real functions with deliberately out-of-range
# size_arg_index values, at both the top level and inside a nested struct
# field) and pointer-field-tracking allocation exhaustion (ptfexhaust_grate.c,
# a 16-byte arena that fits the struct shadow but not the tracking array).
# Separate grates from failclosed_grate.c to avoid symbol-registration
# collisions on toy_buf_checksum (see comments in each grate file).
GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-topindex" \
    "fail-closed/badspec_cage.c" \
    "fail-closed/badspec_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/badspec_cage.cwasm" "topindex" \
    -- "[Cage|badspec] PASS: topindex rejected (GRATE_ERR)" \
    -- \
    -- "[Grate|badspec] memmove handler ran (should not happen)"

GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-nestedindex" \
    "fail-closed/badspec_cage.c" \
    "fail-closed/badspec_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/badspec_cage.cwasm" "nestedindex" \
    -- "[Cage|badspec] PASS: nestedindex rejected (GRATE_ERR)" \
    -- \
    -- "[Grate|badspec] toy_buf_checksum handler ran (should not happen)"

# fail-closed-badnargs: toy_mul's real signature is 2 raw ABI slots (well
# within the transport's 6-slot capacity, so it links normally), but its
# spec deliberately claims .nargs=7 -- lind_marshal_dispatch's own bound on
# spec->nargs must reject this itself, since the linker-level check (which
# only sees the real function's wasm type) cannot.
GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-badnargs" \
    "fail-closed/badspec_cage.c" \
    "fail-closed/badspec_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/badspec_cage.cwasm" "badnargs" \
    -- "[Cage|badspec] PASS: badnargs rejected (GRATE_ERR)" \
    -- \
    -- "[Grate|badspec] toy_mul handler ran (should not happen)"

GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-ptfexhaust" \
    "fail-closed/ptfexhaust_cage.c" \
    "fail-closed/ptfexhaust_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/ptfexhaust_cage.cwasm" \
    -- "[Cage|ptfexhaust] PASS: rejected (GRATE_ERR)" \
    -- \
    -- "[Grate|ptfexhaust] toy_buf_checksum handler ran (should not happen)"

# fail-closed-widesig: toy_wide_sum7 has 7 raw wasm32 ABI slots -- one more
# than the interposition transport's 6-slot capacity. This is rejected at
# link/portal-install time (linker.rs), before any grate delegation, so the
# cage traps on the call itself and can never report its own PASS/FAIL; the
# parent grate judges success from the cage's exit status instead (see
# widesig_grate.c). Distinct from the topindex/nestedindex/ptfexhaust cases
# above, which are rejected inside an already-installed portal's dispatch.
#
# The evidence line requires LIND_LOG_OUTPUT=stderr (set above): without it,
# a bare nonzero cage exit could also mean an unrelated crash, and the test
# would pass for the wrong reason.
GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-widesig" \
    "fail-closed/widesig_cage.c" \
    "fail-closed/widesig_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/widesig_cage.cwasm" \
    -- "[Grate|widesig] registered 1/1 handlers" "[Grate|widesig] PASS: rejected (cage crashed as expected, exit=1)" \
    -- "    1: lib-3i portal: env.toy_wide_sum7 (i32, i32, i32, i32, i32, i32, i32) -> (i32) cannot be transported: 7 raw ABI argument slots, exceeding the interposition transport's 6-slot capacity" \
    -- "[Cage|widesig] FAIL: call returned normally" "[Grate|widesig] toy_wide_sum7 handler ran (should not happen)"

# fail-closed-provenance-*: post-call pointer-provenance validation (issue
# #7). provenance_grate.c's three handlers deliberately fabricate the values
# a buggy or adversarial interposed library might leave behind -- before the
# shadow's base, past its one-past-the-end, wildly out of range, and
# pointing at a real but unrelated grate address -- across the three
# post-call translation paths (LIND_RET_PTR_INTO_ARG, out_ptr_into_arg1, and
# struct-field OUT/cursor copy-back+fixup). Each target loops over every one
# of its modes in a single cage invocation and reports one aggregate PASS
# line; a _lind_marshal_abort trap only fails the one call it's in (see
# threei::GRATE_ERR's doc), so invalid modes don't stop the cage from
# reaching its later, valid ones. The per-mode "handler ran" lines are
# evidence every mode actually executed, not skipped.
GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-provenance-scan" \
    "fail-closed/provenance_cage.c" \
    "fail-closed/provenance_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/provenance_cage.cwasm" "scan" \
    -- "[Grate|provenance] registered 3/3 handlers" "[Cage|provenance] PASS: scan (all 8 modes)" \
    -- "[Grate|provenance] toy_scan_buf handler ran, mode=0" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=1" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=2" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=3" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=4" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=5" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=6" \
       "[Grate|provenance] toy_scan_buf handler ran, mode=7"

GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-provenance-strtol" \
    "fail-closed/provenance_cage.c" \
    "fail-closed/provenance_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/provenance_cage.cwasm" "strtol" \
    -- "[Grate|provenance] registered 3/3 handlers" "[Cage|provenance] PASS: strtol (all 8 modes)" \
    -- "[Grate|provenance] toy_strtol_like handler ran, mode='0'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='1'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='2'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='3'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='4'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='5'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='6'" \
       "[Grate|provenance] toy_strtol_like handler ran, mode='7'"

GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-provenance-stream" \
    "fail-closed/provenance_cage.c" \
    "fail-closed/provenance_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/provenance_cage.cwasm" "stream" \
    -- "[Grate|provenance] registered 3/3 handlers" "[Cage|provenance] PASS: stream (all 7 modes)" \
    -- "[Grate|provenance] toy_stream_process handler ran, mode=0" \
       "[Grate|provenance] toy_stream_process handler ran, mode=1" \
       "[Grate|provenance] toy_stream_process handler ran, mode=2" \
       "[Grate|provenance] toy_stream_process handler ran, mode=3" \
       "[Grate|provenance] toy_stream_process handler ran, mode=4" \
       "[Grate|provenance] toy_stream_process handler ran, mode=5" \
       "[Grate|provenance] toy_stream_process handler ran, mode=6"

# fail-closed-stridevec-*: LIND_SIZE_STRIDE_VECTOR and lind_extent_operand
# (issue #26 review). See stridevec_grate.c/stridevec_cage.c for what each
# mode targets and why. Acceptance modes assert the real handler ran
# (evidence a genuine dispatch happened, not a lucky coincidence);
# rejection modes forbid it (LIND_GRATE_ERR alone doesn't prove the real
# handler never executed).
for mode_desc in \
    "basic:toy_daxpy" \
    "zero:toy_daxpy" \
    "zerostride:toy_daxpy" \
    "pointee:toy_daxpy_ref" \
    "mixed:toy_daxpy_mixed"
do
    mode="${mode_desc%%:*}"
    fn="${mode_desc#*:}"
    GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
    run_test "fail-closed-stridevec-$mode" \
        "fail-closed/stridevec_cage.c" \
        "fail-closed/stridevec_grate.c" \
        "env=/lib/libtoy.so" "yes" \
        "/stridevec_cage.cwasm" "$mode" \
        -- "[Grate|stridevec] registered 4/4 handlers" "[Cage|stridevec] PASS: $mode" \
        -- "[Grate|stridevec] $fn handler ran"
done

for mode_desc in \
    "negstride:toy_daxpy" \
    "overflow:toy_daxpy" \
    "narrow:toy_daxpy" \
    "arenaexhaust:toy_daxpy" \
    "nullpointee:toy_daxpy_ref" \
    "wrongptr:toy_daxpy_ref"
do
    mode="${mode_desc%%:*}"
    fn="${mode_desc#*:}"
    GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
    run_test "fail-closed-stridevec-$mode" \
        "fail-closed/stridevec_cage.c" \
        "fail-closed/stridevec_grate.c" \
        "env=/lib/libtoy.so" "yes" \
        "/stridevec_cage.cwasm" "$mode" \
        -- "[Grate|stridevec] registered 4/4 handlers" "[Cage|stridevec] PASS: $mode" \
        -- \
        -- "[Grate|stridevec] $fn handler ran"
done

GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-stridevec-badindex" \
    "fail-closed/stridevec_cage.c" \
    "fail-closed/stridevec_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/stridevec_cage.cwasm" "badindex" \
    -- "[Grate|stridevec] registered 4/4 handlers" "[Cage|stridevec] PASS: badindex" \
    -- \
    -- "[Grate|stridevec] toy_daxpy_badindex handler ran (should not happen)"

# --------------------------------------------------------------------------
# fail-closed-exprsize-*: LIND_SIZE_EXPR and LIND_SIZE_STRIDE_VECTOR's
# general lind_extent_expr operands (size_operand_expr/stride_operand_expr)
# through the REAL lind_marshal_dispatch path (issue #22's OpenBLAS
# inference-to-runtime integration, Gate 3: "extend generated handler/
# runtime support"). Gate 1's extentexpr-* tests above already exhaustively
# cover the shared evaluator in isolation; these cover the NEW plumbing on
# top of it -- see exprsize_grate.c/exprsize_cage.c for what each mode
# targets and why.
# --------------------------------------------------------------------------
GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
for mode_desc in \
    "basic:toy_daxpy_exprstride" \
    "negstride:toy_daxpy_exprstride" \
    "packedsum:toy_packedsum_exprsize"
do
    mode="${mode_desc%%:*}"
    fn="${mode_desc#*:}"
    GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
    run_test "fail-closed-exprsize-$mode" \
        "fail-closed/exprsize_cage.c" \
        "fail-closed/exprsize_grate.c" \
        "env=/lib/libtoy.so" "yes" \
        "/exprsize_cage.cwasm" "$mode" \
        -- "[Grate|exprsize] registered 3/3 handlers" "[Cage|exprsize] PASS: $mode" \
        -- "[Grate|exprsize] $fn handler ran"
done

GRATE_EXTRA=("$SCRIPT_DIR/custom-lib/libtoy.c")
run_test "fail-closed-exprsize-badtree" \
    "fail-closed/exprsize_cage.c" \
    "fail-closed/exprsize_grate.c" \
    "env=/lib/libtoy.so" "yes" \
    "/exprsize_cage.cwasm" "badtree" \
    -- "[Grate|exprsize] registered 3/3 handlers" "[Cage|exprsize] PASS: badtree" \
    -- \
    -- "[Grate|exprsize] toy_packedsum_badtree handler ran (should not happen)"

# --------------------------------------------------------------------------
# fail-closed-extentexpr-*: the general lind_extent_expr tree and its
# evaluator, _lind_eval_extent_expr (issue #22 / OpenBLAS inference
# integration, Gate 1: "define one runtime extent-expression contract").
# Unlike stridevec above (a single lind_extent_operand leaf), the real
# extent evaluation under test here happens INSIDE toy_extent_probe's own
# handler, not during lind_marshal_dispatch's own pre-call argument
# marshalling -- so, unlike stridevec's reject modes, the handler's own
# "ran" marker is expected evidence for EVERY mode here, accept or reject:
# it proves the handler genuinely reached the tree evaluation, not that
# argument marshalling silently short-circuited beforehand. See
# extentexpr_grate.c for exactly which tree each mode builds.
GRATE_EXTRA=()
for mode_desc in \
    "constant:0" "arg_value:1" "arg_pointee:2" "abs:3" "abs-int-min:19" \
    "product:4" "product-overflow:5" "max:6" "add:7" "add-overflow:8" \
    "ceildiv-round:9" "ceildiv-exact:10" "ceildiv-byzero:11" \
    "ceildiv-negdividend:12" "ceildiv-negdivisor:20" "negative-root:13" \
    "zero-root:1" "sizet-overflow:14" "bad-arg-index:15" "bad-kind:16" \
    "excessive-depth:17" "packed-storage:18" "packed-storage-zero:18" \
    "null-child:21" "const-too-big:22" "pointee-null:23" "pointee-invalid:23" \
    "leaf-u32:24" "leaf-i64:25" "leaf-u64:26" "leaf-u64-overflow:26" \
    "product-negative-operands:27" "node-budget-exceeded:28"
do
    label="${mode_desc%%:*}"
    modenum="${mode_desc#*:}"
    run_test "fail-closed-extentexpr-$label" \
        "fail-closed/extentexpr_cage.c" \
        "fail-closed/extentexpr_grate.c" \
        "env=/lib/libextentexpr_stub.so" "yes" \
        "/extentexpr_cage.cwasm" "$label" \
        -- "[Grate|extentexpr] registered 1/1 handlers" "[Cage|extentexpr] PASS: $label" \
        -- "[Grate|extentexpr] toy_extent_probe handler ran mode=$modenum"
done

DECLARED_TESTS+=("fail-closed")

# --------------------------------------------------------------------------
# exectrace-*: Gate 5's own "strict execution oracle" self-test (issue #22's
# OpenBLAS inference-to-runtime integration) -- its own explicit acceptance
# criterion: "A harness self-test deliberately disables registration and
# proves that a numerical pass is classified as local-only, not
# interposed." exectrace_grate.c registers BOTH a V1 handler (toy_trace_add)
# and a hand-written V2 adapter (toy_trace_sum) in "both" mode, and NEITHER
# in "none" mode; exectrace_stub.c's own real implementation is correct
# either way, so the cage's own PASS/FAIL lines alone can never tell the
# two modes apart -- only lind_marshal.h's own [lind-trace] evidence (built
# with -DLIND_MARSHAL_DEBUG specifically for this test) can, and only
# tools/marshal-gen/execution_oracle.py's classify() turns that evidence
# into an actual PASS_INTERPOSED/PASS_LOCAL_ONLY verdict.
# --------------------------------------------------------------------------
run_exectrace_oracle_proof() {
    local ok=1

    if ! compile_grate "$SCRIPT_DIR/exectrace/exectrace_grate.c" \
            "$SCRIPT_DIR/exectrace/exectrace_stub.c" -DLIND_MARSHAL_DEBUG; then
        fail_test "exectrace-both" build "COMPILE_STEP_FAILED (grate)
$(cat /tmp/lib-interpose-compile.log)"
        fail_test "exectrace-none" build "COMPILE_STEP_FAILED (grate)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi
    if ! compile_src "$SCRIPT_DIR/exectrace/exectrace_cage.c"; then
        fail_test "exectrace-both" build "COMPILE_STEP_FAILED (cage)
$(cat /tmp/lib-interpose-compile.log)"
        fail_test "exectrace-none" build "COMPILE_STEP_FAILED (cage)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi
    if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/exectrace/exectrace_stub.c" \
            > /tmp/lib-interpose-compile.log 2>&1; then
        fail_test "exectrace-both" build "COMPILE_STEP_FAILED (preload stub)
$(cat /tmp/lib-interpose-compile.log)"
        fail_test "exectrace-none" build "COMPILE_STEP_FAILED (preload stub)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi

    mkdir -p "$LINDFS/lib"
    cp "$SCRIPT_DIR/exectrace/exectrace_grate.cwasm" "$GRATES_DIR/"
    cp "$SCRIPT_DIR/exectrace/exectrace_cage.cwasm" "$LINDFS/"
    cp "$SCRIPT_DIR/exectrace/exectrace_stub.so" "$LINDFS/lib/"

    local both_log="/tmp/lib-interpose-exectrace-both.log"
    local none_log="/tmp/lib-interpose-exectrace-none.log"
    local both_output both_exit none_output none_exit
    both_output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" \
        --preload "env=/lib/exectrace_stub.so:interposed" \
        "grates/exectrace_grate.cwasm" both "/exectrace_cage.cwasm" 2>&1)
    both_exit=$?
    printf '%s' "$both_output" > "$both_log"
    none_output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" \
        --preload "env=/lib/exectrace_stub.so" \
        "grates/exectrace_grate.cwasm" none "/exectrace_cage.cwasm" 2>&1)
    none_exit=$?
    printf '%s' "$none_output" > "$none_log"

    rm -f "$GRATES_DIR/exectrace_grate.cwasm" "$LINDFS/exectrace_cage.cwasm" "$LINDFS/lib/exectrace_stub.so"

    # Outputs are read from files, not interpolated into the script below,
    # so arbitrary content (quotes, backslashes, stray """) in a grate's
    # own stderr can never corrupt or inject into this classification step.
    #
    # classify()'s own "any required symbol observed" semantics (correct,
    # and deliberately kept, for a REAL OpenBLAS test, where any one of
    # several equivalent symbols proving coverage is enough) would let
    # "both" mode pass this self-test even if only ONE of the two
    # transports actually traced -- a V2-only or V1-only regression could
    # hide behind the other transport's own evidence. So this self-test
    # additionally asserts each symbol's own EXACT call count AND the real
    # V2 pointer-size trace line's own fields, independently proving BOTH
    # paths (not just "at least one"), end to end.
    local verdicts
    verdicts=$(python3 - "$both_exit" "$none_exit" "$both_log" "$none_log" <<PYEOF
import sys
sys.path.insert(0, "$REPO_ROOT/tools/marshal-gen")
import execution_oracle as oracle

both_exit, none_exit, both_log, none_log = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
with open(both_log) as fh:
    both_output = fh.read()
with open(none_log) as fh:
    none_output = fh.read()

def numeric_ok(output):
    return "[Cage|exectrace] PASS: toy_trace_add" in output and "[Cage|exectrace] PASS: toy_trace_sum" in output

both_verdict = oracle.classify(
    required_symbols=("toy_trace_add", "toy_trace_sum"),
    numeric_ok=(both_exit == 0 and numeric_ok(both_output)),
    call_counts=oracle.parse_call_counts(both_output))
none_verdict = oracle.classify(
    required_symbols=("toy_trace_add", "toy_trace_sum"),
    numeric_ok=(none_exit == 0 and numeric_ok(none_output)),
    call_counts=oracle.parse_call_counts(none_output))

both_counts = oracle.parse_call_counts(both_output)
both_ptr_sizes = oracle.parse_ptr_sizes(both_output)

print(both_verdict)
print(none_verdict)
print(both_counts.get("toy_trace_add", 0))
print(both_counts.get("toy_trace_sum", 0))
print(1 if ("toy_trace_sum", "const", 20) in both_ptr_sizes else 0)
PYEOF
)
    rm -f "$both_log" "$none_log"
    local both_verdict none_verdict both_add_count both_sum_count both_ptr_ok
    both_verdict="$(sed -n '1p' <<<"$verdicts")"
    none_verdict="$(sed -n '2p' <<<"$verdicts")"
    both_add_count="$(sed -n '3p' <<<"$verdicts")"
    both_sum_count="$(sed -n '4p' <<<"$verdicts")"
    both_ptr_ok="$(sed -n '5p' <<<"$verdicts")"

    local both_errors=()
    [[ "$both_verdict" != "PASS_INTERPOSED" ]] && \
        both_errors+=("expected classify()==PASS_INTERPOSED, got '$both_verdict'")
    [[ "$both_add_count" != "1" ]] && \
        both_errors+=("expected toy_trace_add (V1) observed exactly once, got $both_add_count -- the V1 transport path may not have traced")
    [[ "$both_sum_count" != "1" ]] && \
        both_errors+=("expected toy_trace_sum (V2) observed exactly once, got $both_sum_count -- the V2 transport path may not have traced")
    [[ "$both_ptr_ok" != "1" ]] && \
        both_errors+=("expected a [lind-trace] toy_trace_sum ptr size_kind=const bytes=0x14 line -- the V2 pointer-size trace may be missing or wrong")

    if [[ ${#both_errors[@]} -gt 0 ]]; then
        fail_test "exectrace-both" assertion "$(printf '  - %s\n' "${both_errors[@]}")
--- actual output ---
$both_output"
        ok=0
    else
        pass_test "exectrace-both"
    fi

    if [[ "$none_verdict" != "PASS_LOCAL_ONLY" ]]; then
        fail_test "exectrace-none" assertion "expected classify()==PASS_LOCAL_ONLY, got '$none_verdict'
--- actual output ---
$none_output"
        ok=0
    else
        pass_test "exectrace-none"
    fi
    return $(( ok == 1 ? 0 : 1 ))
}

GRATE_EXTRA=()
run_exectrace_oracle_proof
DECLARED_TESTS+=("exectrace-both" "exectrace-none" "exectrace")

# --------------------------------------------------------------------------
# fail-registration: a grate must abort startup rather than exec the cage
# with an incomplete handler table. One registration is deliberately given
# a NULL symbol name (register_lib_handler rejects it); the grate must
# detect the failure and abort before execv. The payload path
# (/custom-lib.cwasm) is a placeholder reused from an existing test binary
# and is never actually run -- if it were, that would itself be the bug
# this test exists to catch.
# --------------------------------------------------------------------------
GRATE_EXTRA=()
run_test "fail-registration" \
    "custom-lib/custom-lib.c" \
    "fail-registration/failreg_grate.c" \
    "" "no" \
    "/custom-lib.cwasm" \
    -- "[Grate|fail-registration] register (null symbol) failed: -1" \
       "[Grate|fail-registration] registered 1/2 handlers" \
       "[Grate|fail-registration] FATAL: 1 handler registration(s) failed, aborting startup" \
    --

# --------------------------------------------------------------------------
# auto-openblas-daxpy: cblas_daxpy/daxpy_ V1 handlers generated by
# gen_grate.py straight from the live openblas.marshal.json (issue #26/#27
# follow-up, items 6-7) -- proves a REAL, contract-backed StrideVector
# inference record is actually usable through V1 generation (gen_grate.py)
# and a real compiled-and-run grate's DISPATCH/marshalling machinery, not
# just through the hand-written specs fail-closed/stridevec_grate.c uses to
# isolate the evaluator. Multiple elements and non-unit strides on both
# arrays (see daxpy_cage.c). Strict-safe: the cage calls only
# cblas_daxpy/daxpy_ from libblastoy.
#
# NOT a real-OpenBLAS-execution proof: libblastoy.c is a hand-written
# stand-in sharing OpenBLAS's real exported symbol names and raw wasm32 ABI
# shapes, not the real statically-linked libopenblas.a -- see that file's
# own comment for why (V1's --compile-grate/--fpcast-emu path predates a
# proven real-archive link; the toy keeps this test's numeric expectations
# hand-derivable). The REAL libopenblas.a, real cblas_daxpby/daxpby_
# implementation, and a same-cage numeric baseline are what
# auto-openblas-v2wide-real / auto-openblas-v2wide-fortran-real below prove
# instead -- those are the only two OpenBLAS symbols
# currently exercised against the real archive.
# --------------------------------------------------------------------------
if [[ "$OPENBLAS_DAXPY_GEN_OK" == "yes" ]]; then
    GRATE_EXTRA=("$SCRIPT_DIR/auto-openblas-daxpy/libblastoy.c")
    run_test "auto-openblas-daxpy" \
        "auto-openblas-daxpy/daxpy_cage.c" \
        "auto-openblas-daxpy/openblas_daxpy_auto_grate.c" \
        "env=/lib/libblastoy.so" "yes" \
        "/daxpy_cage.cwasm" \
        -- "[libblastoy-grate] registered 2/2 handlers" \
           "[Cage|openblas-daxpy] PASS: cblas_daxpy" "[Cage|openblas-daxpy] PASS: daxpy_" \
        -- "[libblastoy] cblas_daxpy handler ran n=5 incx=2 incy=3" \
           "[libblastoy] daxpy_ handler ran n=5 incx=2 incy=3"
else
    DECLARED_TESTS+=("auto-openblas-daxpy")
    skip_test "auto-openblas-daxpy" "$OPENBLAS_JSON not found (run tools/marshal-infer/infer_openblas.sh first)"
fi

# --------------------------------------------------------------------------
# auto-openblas-v2wide-real / auto-openblas-v2wide-fortran-real: a REAL,
# wide (7-raw-ABI-slot) OpenBLAS
# daxpby, statically linked from the REAL libopenblas.a, interposed through
# the REAL V2 production path (register_lib_handler_v2 +
# Linker::instance_dylink's own V2 portal check + wasmtime_lind_3i's
# worker-pool integration -- the same machinery auto-v2wide-real-* proved
# against a hand-written toy function, now exercised against a real,
# previously-inaccessible OpenBLAS export). Both the CBLAS by-value form
# (cblas_daxpby, LIND_EXTENT_VALUE) and the classic Fortran-BLAS
# by-reference form (daxpby_, LIND_EXTENT_POINTEE_I32) are proven -- the two
# StrideVector extent-source conventions this project supports, not just
# one of them.
#
# Neither uses run_test: the proof here is that the interposed run's
# numeric output agrees BIT-FOR-BIT (%a hex-float) with a same-cage
# baseline -- a plain, non-interposed program statically linking the SAME
# libopenblas.a and calling the real function directly -- which needs
# comparing two separate runs' output against each other, not a fixed
# expected-value list.
#
# run_openblas_v2wide_proof <test_name> <baseline_src> <grate_src> <cage_src>
#     <stub_so_name> <tag> <expect_registered_line> <stub_fail_marker>
# `tag` is the common "[Baseline|<tag>]"/"[Cage|<tag>]" prefix both the
# baseline and cage source files print their y[] values with.
# run_v2_handle_trap_test <name> <mode> <evidence...>
# A bespoke check (not run_test) for a handle_v2_real_cage.c mode that must
# TRAP the whole process rather than return a soft GRATE_ERR sentinel --
# lind_marshal.h's own LIND_ARG_HANDLE check traps on an untranslatable
# token instead of returning a checkable value (see that file's "a nonzero
# token that fails to translate traps" comment), so run_test's own "exit 0,
# check printed lines" convention cannot express this case: a NONZERO exit
# here is the expected, correct outcome. Compiles/stages/runs the same way
# run_test does; only the pass/fail decision differs.
run_v2_handle_trap_test() {
    local name="$1" mode="$2"; shift 2
    local evidence=("$@")

    DECLARED_TESTS+=("$name")

    if ! compile_src "$SCRIPT_DIR/auto-v2wide/handle_v2_real_cage.c"; then
        fail_test "$name" build "COMPILE_STEP_FAILED (cage)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi
    GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_handle_impl.c")
    if ! compile_grate "$SCRIPT_DIR/auto-v2wide/handle_v2_grate.c" "${GRATE_EXTRA[@]}"; then
        fail_test "$name" build "COMPILE_STEP_FAILED (grate)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi

    cp "$SCRIPT_DIR/auto-v2wide/handle_v2_grate.cwasm" "$GRATES_DIR/"
    cp "$SCRIPT_DIR/auto-v2wide/handle_v2_real_cage.cwasm" "$LINDFS/"

    local output exit_code
    output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" \
        --preload "env=/lib/libtoy_handle_v2_stub.so:interposed" \
        "grates/handle_v2_grate.cwasm" "handle_v2_real_cage.cwasm" "$mode" 2>&1)
    exit_code=$?

    rm -f "$GRATES_DIR/handle_v2_grate.cwasm" "$LINDFS/handle_v2_real_cage.cwasm"

    local missing=() m line
    if [[ ${#evidence[@]} -gt 0 ]] && ! m="$(match_ordered_lines "$output" "${evidence[@]}")"; then
        while IFS= read -r line; do missing+=("$line"); done <<<"$m"
    fi
    if ! m="$(find_forbidden_lines "$output" \
            "[Cage|handle-v2] FAIL: wrong-class token was not rejected" \
            "[Cage|handle-v2] FAIL: stale token was not rejected" \
            "[libtoy_handle_v2_stub] FAIL: real (uninterposed) toy_ctx_get_val_v2 ran")"; then
        while IFS= read -r line; do missing+=("[forbidden, but present] $line"); done <<<"$m"
    fi

    if [[ "$exit_code" -eq 0 ]]; then
        fail_test "$name" semantic "expected a nonzero (trapped) exit code, got 0
--- actual output ---
$output"
    elif [[ ${#missing[@]} -gt 0 ]]; then
        fail_test "$name" "$(category_for "$output" "$exit_code")" \
            "validation failed:
$(printf '  - %s\n' "${missing[@]}")
--- actual output ---
$output"
    else
        pass_test "$name"
    fi
}

run_openblas_v2wide_proof() {
    local test_name="$1" baseline_src="$2" grate_src="$3" cage_src="$4"
    local stub_so="$5" tag="$6" expect_registered="$7" stub_fail_marker="$8"
    local baseline_cwasm grate_cwasm cage_cwasm

    if ! "$LIND_COMPILE" -s "$SCRIPT_DIR/$baseline_src" -- "$OPENBLAS_A" \
            > /tmp/lib-interpose-compile.log 2>&1; then
        fail_test "$test_name" build "COMPILE_STEP_FAILED (baseline)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi
    baseline_cwasm="$(basename "${baseline_src%.c}").cwasm"
    cp "$SCRIPT_DIR/$(dirname "$baseline_src")/$baseline_cwasm" "$LINDFS/$baseline_cwasm"
    local baseline_output baseline_exit
    baseline_output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" "$baseline_cwasm" 2>&1)
    baseline_exit=$?
    rm -f "$LINDFS/$baseline_cwasm"

    if ! compile_grate "$SCRIPT_DIR/$grate_src" \
            "$SCRIPT_DIR/auto-openblas-v2wide/daxpby_v2_adapter.c" "$OPENBLAS_A"; then
        fail_test "$test_name" build "COMPILE_STEP_FAILED (grate)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi
    if ! compile_src "$SCRIPT_DIR/$cage_src"; then
        fail_test "$test_name" build "COMPILE_STEP_FAILED (cage)
$(cat /tmp/lib-interpose-compile.log)"
        return
    fi

    grate_cwasm="$(basename "${grate_src%.c}").cwasm"
    cage_cwasm="$(basename "${cage_src%.c}").cwasm"
    cp "$SCRIPT_DIR/$(dirname "$grate_src")/$grate_cwasm" "$GRATES_DIR/"
    cp "$SCRIPT_DIR/$(dirname "$cage_src")/$cage_cwasm" "$LINDFS/"
    local interposed_output interposed_exit
    interposed_output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" \
        --preload "env=/lib/$stub_so:interposed" \
        "grates/$grate_cwasm" "/$cage_cwasm" 2>&1)
    interposed_exit=$?
    rm -f "$GRATES_DIR/$grate_cwasm" "$LINDFS/$cage_cwasm"

    local v2wide_missing=() m line
    if ! m="$(match_ordered_lines "$interposed_output" \
            "$expect_registered" \
            "[Grate|$tag] app exited 0")"; then
        while IFS= read -r line; do v2wide_missing+=("$line"); done <<<"$m"
    fi
    # A silent interposition bypass would fall through to the stub's own
    # body instead of the real, statically-linked implementation --
    # forbidding its marker line rules that out (see each stub's own
    # comment).
    if ! m="$(find_forbidden_lines "$interposed_output" \
            "$stub_fail_marker")"; then
        while IFS= read -r line; do v2wide_missing+=("[forbidden, but present] $line"); done <<<"$m"
    fi

    local baseline_y interposed_y
    baseline_y="$(grep -E "^\[Baseline\|$tag\] y\[" <<<"$baseline_output" | sed -E 's/^\[[^]]+\] //')"
    interposed_y="$(grep -E "^\[Cage\|$tag\] y\[" <<<"$interposed_output" | sed -E 's/^\[[^]]+\] //')"

    if [[ "$baseline_exit" -ne 0 || -z "$baseline_y" ]]; then
        fail_test "$test_name" "$(category_for "$baseline_output" "$baseline_exit")" \
            "same-cage baseline failed to produce output:
--- baseline output ---
$baseline_output"
    elif [[ "$interposed_exit" -ne 0 || ${#v2wide_missing[@]} -gt 0 ]]; then
        fail_test "$test_name" "$(category_for "$interposed_output" "$interposed_exit")" \
            "validation failed:
$(printf '  - %s\n' "${v2wide_missing[@]}")
--- actual output ---
$interposed_output"
    elif [[ "$baseline_y" != "$interposed_y" ]]; then
        fail_test "$test_name" assertion \
            "same-cage baseline and V2-interposed run disagree:
--- baseline (uninterposed, direct call) ---
$baseline_y
--- interposed (real V2 production path) ---
$interposed_y"
    else
        pass_test "$test_name"
    fi
}

DECLARED_TESTS+=("auto-openblas-v2wide")
if [[ "$OPENBLAS_V2_GEN_OK" == "yes" ]]; then
    run_openblas_v2wide_proof "auto-openblas-v2wide-real" \
        "auto-openblas-v2wide/daxpby_baseline.c" \
        "auto-openblas-v2wide/daxpby_v2_real_grate.c" \
        "auto-openblas-v2wide/daxpby_v2_real_cage.c" \
        "libdaxpby_v2_stub.so" "daxpby-v2" \
        "[Grate|daxpby-v2] registered 1/1 handlers" \
        "[libdaxpby_v2_stub] FAIL: real (uninterposed) implementation ran"

    run_openblas_v2wide_proof "auto-openblas-v2wide-fortran-real" \
        "auto-openblas-v2wide/daxpby_fortran_baseline.c" \
        "auto-openblas-v2wide/daxpby_fortran_v2_real_grate.c" \
        "auto-openblas-v2wide/daxpby_fortran_v2_real_cage.c" \
        "libdaxpby_fortran_v2_stub.so" "daxpby-fortran-v2" \
        "[Grate|daxpby-fortran-v2] registered 1/1 handlers" \
        "[libdaxpby_fortran_v2_stub] FAIL: real (uninterposed) implementation ran"
else
    skip_test "auto-openblas-v2wide-real" \
        "needs both $OPENBLAS_JSON and $OPENBLAS_A (run tools/marshal-infer/infer_openblas.sh and lind-wasm-apps/openblas/compile_openblas.sh first)"
    skip_test "auto-openblas-v2wide-fortran-real" \
        "needs both $OPENBLAS_JSON and $OPENBLAS_A (run tools/marshal-infer/infer_openblas.sh and lind-wasm-apps/openblas/compile_openblas.sh first)"
fi

# --------------------------------------------------------------------------
# auto-v2wide-abi: V2 generator/inference coverage for two ABI shapes that
# previously had no real-toolchain V2 proof
# before now: a hidden sret return and a byval aggregate argument.
# combine_impl.c's combine_sret_byval takes a 32-byte struct BY VALUE
# (clang lowers this to a `byval` pointer argument) and returns a 12-byte
# struct BY VALUE (lowered to a hidden leading `sret` pointer argument),
# plus 5 plain ints -- 7 raw ABI slots total, V2-only by construction, the
# same shape class as auto-openblas-v2wide's daxpby but exercising two ABI
# lowering shapes the scalar-only WAT resolution tests do not cover
# (those covered scalar-only signatures). Inference and generation both run
# fresh from REAL `lind_compile --emit-marshal` + gen_v2_adapter.py output
# every run, not a hand-authored JSON spec -- see combine_impl.c's own
# comment. combine_v2_real_grate.c's hand-typed signature descriptor
# ("1:iiiiiii:") is independently derived from the real lowered type and
# cross-checked against the generated adapter's own signature; a mismatch
# would make V2AdapterCache::resolve's SignatureMismatch check
# reject this registration at first-call resolution instead of letting it
# dispatch, so this test's own success IS the proof the two agree.
echo "Generating auto-v2wide-abi/combine_impl.marshal.json (real inference)"
if ! "$LIND_COMPILE" --emit-marshal "$SCRIPT_DIR/auto-v2wide-abi/combine_impl.c" \
        > /tmp/lib-interpose-gen-combine-infer.log 2>&1; then
    echo "FATAL: lind_compile --emit-marshal failed on auto-v2wide-abi/combine_impl.c:" >&2
    cat /tmp/lib-interpose-gen-combine-infer.log >&2
    exit 1
fi
echo "Generating auto-v2wide-abi/combine_v2_adapter.c from combine_impl.marshal.json"
if ! python3 "$REPO_ROOT/tools/marshal-gen/gen_v2_adapter.py" \
        "$SCRIPT_DIR/auto-v2wide-abi/combine_impl.marshal.json" \
        --lib-name combine --only combine_sret_byval --manifest-version 1 \
        --out "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_adapter.c" \
        > /tmp/lib-interpose-gen-combine-v2.log 2>&1; then
    echo "FATAL: gen_v2_adapter.py failed to generate the combine_sret_byval V2 adapter:" >&2
    cat /tmp/lib-interpose-gen-combine-v2.log >&2
    exit 1
fi
if ! grep -q "__lind_v2_adapter_combine_sret_byval" "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_adapter.c"; then
    echo "FATAL: generated V2 adapter is missing __lind_v2_adapter_combine_sret_byval" >&2
    exit 1
fi
echo ""
echo "Building shared fixture: libcombine_v2_stub.so"
if ! "$LIND_COMPILE" --compile-library "$SCRIPT_DIR/auto-v2wide-abi/libcombine_v2_stub.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    echo "FATAL: failed to build auto-v2wide-abi/libcombine_v2_stub.c:"
    cat /tmp/lib-interpose-compile.log
    exit 2
fi
cp "$SCRIPT_DIR/auto-v2wide-abi/libcombine_v2_stub.so" "$LINDFS/lib/libcombine_v2_stub.so"
echo ""

DECLARED_TESTS+=("auto-v2wide-abi")
combine_v2_name="auto-v2wide-abi-real"
if ! "$LIND_COMPILE" -s "$SCRIPT_DIR/auto-v2wide-abi/combine_baseline.c" -- \
        "$SCRIPT_DIR/auto-v2wide-abi/combine_impl.c" \
        > /tmp/lib-interpose-compile.log 2>&1; then
    fail_test "$combine_v2_name" build "COMPILE_STEP_FAILED (baseline)
$(cat /tmp/lib-interpose-compile.log)"
else
    cp "$SCRIPT_DIR/auto-v2wide-abi/combine_baseline.cwasm" "$LINDFS/combine_baseline.cwasm"
    combine_baseline_output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" combine_baseline.cwasm 2>&1)
    combine_baseline_exit=$?
    rm -f "$LINDFS/combine_baseline.cwasm"

    if ! compile_grate "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_real_grate.c" \
            "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_adapter.c" \
            "$SCRIPT_DIR/auto-v2wide-abi/combine_impl.c"; then
        fail_test "$combine_v2_name" build "COMPILE_STEP_FAILED (grate)
$(cat /tmp/lib-interpose-compile.log)"
    elif ! compile_src "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_real_cage.c"; then
        fail_test "$combine_v2_name" build "COMPILE_STEP_FAILED (cage)
$(cat /tmp/lib-interpose-compile.log)"
    else
        cp "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_real_grate.cwasm" "$GRATES_DIR/"
        cp "$SCRIPT_DIR/auto-v2wide-abi/combine_v2_real_cage.cwasm" "$LINDFS/"
        combine_interposed_output=$(cd "$LINDFS" && timeout 30 "$LIND_RUN" \
            --preload "env=/lib/libcombine_v2_stub.so:interposed" \
            "grates/combine_v2_real_grate.cwasm" "/combine_v2_real_cage.cwasm" 2>&1)
        combine_interposed_exit=$?
        rm -f "$GRATES_DIR/combine_v2_real_grate.cwasm" "$LINDFS/combine_v2_real_cage.cwasm"

        combine_v2_missing=()
        if ! m="$(match_ordered_lines "$combine_interposed_output" \
                "[Grate|combine-v2] registered 1/1 handlers" \
                "[Grate|combine-v2] app exited 0")"; then
            while IFS= read -r line; do combine_v2_missing+=("$line"); done <<<"$m"
        fi
        if ! m="$(find_forbidden_lines "$combine_interposed_output" \
                "[libcombine_v2_stub] FAIL: real (uninterposed) implementation ran")"; then
            while IFS= read -r line; do combine_v2_missing+=("[forbidden, but present] $line"); done <<<"$m"
        fi

        combine_baseline_r="$(grep -E '^\[Baseline\|combine-v2\] r\.' <<<"$combine_baseline_output" | sed -E 's/^\[[^]]+\] //')"
        combine_interposed_r="$(grep -E '^\[Cage\|combine-v2\] r\.' <<<"$combine_interposed_output" | sed -E 's/^\[[^]]+\] //')"

        if [[ "$combine_baseline_exit" -ne 0 || -z "$combine_baseline_r" ]]; then
            fail_test "$combine_v2_name" "$(category_for "$combine_baseline_output" "$combine_baseline_exit")" \
                "same-cage baseline failed to produce output:
--- baseline output ---
$combine_baseline_output"
        elif [[ "$combine_interposed_exit" -ne 0 || ${#combine_v2_missing[@]} -gt 0 ]]; then
            fail_test "$combine_v2_name" "$(category_for "$combine_interposed_output" "$combine_interposed_exit")" \
                "validation failed:
$(printf '  - %s\n' "${combine_v2_missing[@]}")
--- actual output ---
$combine_interposed_output"
        elif [[ "$combine_baseline_r" != "$combine_interposed_r" ]]; then
            fail_test "$combine_v2_name" assertion \
                "same-cage baseline and V2-interposed run disagree:
--- baseline (uninterposed, direct call) ---
$combine_baseline_r
--- interposed (real V2 production path) ---
$combine_interposed_r"
        else
            pass_test "$combine_v2_name"
        fi
    fi
fi

# --------------------------------------------------------------------------
# auto-conststride: toy_vec_scale handler generated by gen_grate.py from a
# fresh inference run, exercising a constant-sourced StrideVector extent
# operand (ExtentSource::Constant/LIND_EXTENT_CONSTANT) end to end -- an
# ordinary contiguous `x[i]` walk with no separate increment argument at
# all, proven directly from the loop's own IR. Multiple elements (n=5) are
# required: n==1 can't distinguish a correct constant-stride extent
# computation spanning all n elements from an accidental single-element
# copy. Strict-safe: the cage calls only toy_vec_scale from libconststride.
# --------------------------------------------------------------------------
GRATE_EXTRA=("$SCRIPT_DIR/auto-conststride/libconststride.c")
run_test "auto-conststride" \
    "auto-conststride/conststride_cage.c" \
    "auto-conststride/conststride_auto_grate.c" \
    "env=/lib/libconststride.so" "yes" \
    "/conststride_cage.cwasm" \
    -- "[libconststride-grate] registered 1/1 handlers" \
       "[Cage|conststride] PASS: toy_vec_scale" \
    -- "[libconststride] toy_vec_scale handler ran n=5"

# --------------------------------------------------------------------------
# auto-conststride-v2: the SAME toy_vec_scale marshal record, generated by
# gen_v2_adapter.py --emit-grate instead of gen_grate.py -- proving the V2
# self-contained-grate generator is a drop-in replacement
# for gen_grate.py's V1 GRATE_TEMPLATE even for a function that already fits
# V1's six-slot transport, not just the wide functions V1 could never carry.
# Registers through register_lib_handler_v2/the real V2 production path
# (Linker::instance_dylink's V2 portal, GrateWorker::run_v2), no V1
# transport involved anywhere. Strict-safe: the cage calls only
# toy_vec_scale from libconststride.
# --------------------------------------------------------------------------
GRATE_EXTRA=("$SCRIPT_DIR/auto-conststride/libconststride.c")
run_test "auto-conststride-v2" \
    "auto-conststride/conststride_cage.c" \
    "auto-conststride/conststride_v2_grate.c" \
    "env=/lib/libconststride.so" "yes" \
    "/conststride_cage.cwasm" \
    -- "[libconststride-v2-grate] registered 1/1 handlers" \
       "[Cage|conststride] PASS: toy_vec_scale" \
    -- "[libconststride] toy_vec_scale handler ran n=5"

# --------------------------------------------------------------------------
# auto-v2wide: exercises tools/marshal-gen/gen_v2_adapter.py's generated
# variable-width adapter for a 9-logical-argument function (three past V1's
# fixed six-slot dispatch limit) end to end: scalar, pointer IN, pointer
# INOUT, a handle, pointer OUT, and a pointer-alias return, all marshalled
# by the same shared prepare/finish/translate-return primitives V1's own
# dispatch uses. v2wide_shim_grate.c's own outer V1 transport is a single
# struct-pointer argument -- a way to get a real, cross-cage-addressable
# second cage talking to the generated adapter without a full V2 syscall/
# portal/worker path; the six-slot limit that shim itself has is not what
# is under test. Strict-safe: the cage calls only toy_wide_marshal from
# libtoy_wide_stub.
#
# Acceptance modes assert the generated adapter's real handler genuinely
# ran (evidence a genuine dispatch happened, not a lucky coincidence);
# rejection modes forbid it -- a caller-visible rejection alone does not
# prove the real handler never executed.
GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
for mode_desc in \
    "basic:pass" \
    "wrongoutptr:pass"
do
    mode="${mode_desc%%:*}"
    run_test "auto-v2wide-$mode" \
        "auto-v2wide/v2wide_cage.c" \
        "auto-v2wide/v2wide_shim_grate.c" \
        "env=/lib/libtoy_wide_stub.so" "yes" \
        "/v2wide_cage.cwasm" "$mode" \
        -- "[Grate|v2wide] registered 1/1 handlers" "[Cage|v2wide] PASS: $mode" \
        -- "[Grate|v2wide] toy_wide_shim ran" "[Grate|v2wide-real] toy_wide_marshal handler ran"
done

for mode in narrow badtoken; do
    GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
    run_test "auto-v2wide-$mode" \
        "auto-v2wide/v2wide_cage.c" \
        "auto-v2wide/v2wide_shim_grate.c" \
        "env=/lib/libtoy_wide_stub.so" "yes" \
        "/v2wide_cage.cwasm" "$mode" \
        -- "[Grate|v2wide] registered 1/1 handlers" "[Cage|v2wide] PASS: $mode" \
        -- "[Grate|v2wide] toy_wide_shim ran" \
        -- "[Grate|v2wide-real] toy_wide_marshal handler ran"
done

# --------------------------------------------------------------------------
# auto-v2wide-real-*: the SAME generated adapter, exercised through the
# real V2 (variable-width) production path instead of v2wide_shim_grate.c's
# V1-shim workaround -- register_lib_handler_v2,
# Linker::instance_dylink's own V2 portal check, and wasmtime_lind_3i's
# worker-pool integration (GrateWorker::run_v2/GrateHandler::submit_v2),
# with no V1 transport involved anywhere. v2wide_real_cage.c calls
# toy_wide_marshal directly with its real 9-argument signature -- no six-slot
# limit anywhere in this path at all, unlike the shim's own outer transport.
# Strict-safe: the cage calls only toy_wide_marshal from libtoy_wide_real_stub.
GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
for mode_desc in \
    "basic:pass" \
    "sequence:pass" \
    "fork:pass" \
    "concurrent:pass"
do
    mode="${mode_desc%%:*}"
    run_test "auto-v2wide-real-$mode" \
        "auto-v2wide/v2wide_real_cage.c" \
        "auto-v2wide/v2wide_real_grate.c" \
        "env=/lib/libtoy_wide_real_stub.so" "yes" \
        "/v2wide_real_cage.cwasm" "$mode" \
        -- "[Grate|v2wide-real] registered 1/1 handlers" "[Cage|v2wide-real] PASS: $mode" \
        -- "[Grate|v2wide-real] toy_wide_marshal handler ran"
done

GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
run_test "auto-v2wide-real-narrow" \
    "auto-v2wide/v2wide_real_cage.c" \
    "auto-v2wide/v2wide_real_grate.c" \
    "env=/lib/libtoy_wide_real_stub.so" "yes" \
    "/v2wide_real_cage.cwasm" "narrow" \
    -- "[Grate|v2wide-real] registered 1/1 handlers" "[Cage|v2wide-real] PASS: narrow" \
    -- \
    -- "[Grate|v2wide-real] toy_wide_marshal handler ran"

# --------------------------------------------------------------------------
# auto-v2wide-real-stale: v2wide_stale_grate.c registers toy_wide_marshal
# against a grate cage that has ALREADY exited and been reaped before the
# app cage attempting the call even starts, proving a grate exit followed
# by a stale call is rejected cleanly by dispatch_lib_call_v2's existing
# cage-liveness check
# instead of hanging or crashing. The real handler must never run: a
# rejection that wrongly let the call through would still print the same
# GRATE_ERR sentinel by coincidence if something else went wrong, so
# forbidding the "handler ran" marker is required here too, same reasoning
# as auto-v2wide-real-narrow.
GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
run_test "auto-v2wide-real-stale" \
    "auto-v2wide/v2wide_real_cage.c" \
    "auto-v2wide/v2wide_stale_grate.c" \
    "env=/lib/libtoy_wide_real_stub.so" "yes" \
    "/v2wide_real_cage.cwasm" "stale" \
    -- "[Grate|v2wide-real] registered 1/1 handlers" "[Cage|v2wide-real] PASS: stale" \
    -- \
    -- "[Grate|v2wide-real] toy_wide_marshal handler ran"

# --------------------------------------------------------------------------
# auto-v2wide-handle-*: LIND_RET_HANDLE/LIND_ARG_HANDLE through a FULLY
# GENERATED V2 grate (gen_v2_adapter.py --emit-grate), not a hand-written
# handler. Covers successful handle round-trip plus wrong-class/stale-token
# rejection. Strict-safe: the cage calls
# only handle_v2's own symbols, all covered by libtoy_handle_v2_stub.
GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_handle_impl.c")
run_test "auto-v2wide-handle-roundtrip" \
    "auto-v2wide/handle_v2_real_cage.c" \
    "auto-v2wide/handle_v2_grate.c" \
    "env=/lib/libtoy_handle_v2_stub.so" "yes" \
    "/handle_v2_real_cage.cwasm" "roundtrip" \
    -- "[Cage|handle-v2] PASS: roundtrip, val=42" \
    -- "[Grate|handle-v2] toy_ctx_create_v2 ran val=42" \
       "[Grate|handle-v2] toy_ctx_get_val_v2 ran val=42" \
       "[Grate|handle-v2] toy_ctx_close_v2 ran"

run_v2_handle_trap_test "auto-v2wide-handle-wrongclass" "wrongclass" \
    "[Grate|handle-v2] toy_ctx_create_v2 ran val=42" \
    "[Grate|handle-v2] toy_ctx_get_val_v2 ran val=42" \
    "[Grate|handle-v2] toy_other_create_v2 ran val=7" \
    "[Cage|handle-v2] about to access wrong-class token"

run_v2_handle_trap_test "auto-v2wide-handle-stale" "stale" \
    "[Grate|handle-v2] toy_ctx_create_v2 ran val=42" \
    "[Grate|handle-v2] toy_ctx_get_val_v2 ran val=42" \
    "[Grate|handle-v2] toy_ctx_close_v2 ran" \
    "[Cage|handle-v2] about to access stale token"

# --------------------------------------------------------------------------
# auto-v2wide-real-errno: toy_set_errno's real handler runs inside the
# GRATE's own address space -- proving the V2 portal's errno seed/relay
# (linker.rs's seed_grate_errno_from_caller/relay_grate_errno_to_caller,
# SHARED with the V1 portal, not a separate implementation) carries that
# write back into the CALLING cage's own errno slot. Uses
# v2wide_errno_grate.c, the only grate that registers toy_set_errno
# (isolated from v2wide_real_grate.c so this doesn't perturb its own
# "registered 1/1 handlers" tests).
GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
run_test "auto-v2wide-real-errno" \
    "auto-v2wide/v2wide_real_cage.c" \
    "auto-v2wide/v2wide_errno_grate.c" \
    "env=/lib/libtoy_wide_real_stub.so" "yes" \
    "/v2wide_real_cage.cwasm" "errno" \
    -- "[Grate|v2wide-errno] registered 1/1 handlers" "[Cage|v2wide-real] PASS: errno" \
    -- "[Grate|v2wide-real] toy_set_errno ran val=4242"

# --------------------------------------------------------------------------
# auto-v2wide-real-exec: a V2 registration survives exec(). The cage makes
# one successful call, execs a fresh copy of itself (same cage id, new
# program image), and makes the SAME call again post-exec -- proving
# Linker::instance_dylink's V2 portal check for the NEW image still finds
# the registration recorded under this cage's id before the original exec
# into this program (lib_handler_table_v2 is keyed by cage id, which exec
# does not change).
GRATE_EXTRA=("$SCRIPT_DIR/auto-v2wide/toy_impl.c" "$SCRIPT_DIR/auto-v2wide/v2wide_adapters.c")
run_test "auto-v2wide-real-exec" \
    "auto-v2wide/v2wide_real_cage.c" \
    "auto-v2wide/v2wide_real_grate.c" \
    "env=/lib/libtoy_wide_real_stub.so" "yes" \
    "/v2wide_real_cage.cwasm" "exec" \
    -- "[Grate|v2wide-real] registered 1/1 handlers" \
       "[Cage|v2wide-real] about to exec self" \
       "[Cage|v2wide-real] PASS: exec" \
    -- "[Grate|v2wide-real] toy_wide_marshal handler ran"

DECLARED_TESTS+=("auto-v2wide")

# --------------------------------------------------------------------------
# Completeness check: every directory with a *_grate.c must be declared
# above. full-libc/ and full-libm/ are separate, much larger suites with
# their own dedicated runners and are intentionally excluded here.
# --------------------------------------------------------------------------
echo ""
missing_from_manifest=()
while IFS= read -r dir; do
    missing_from_manifest+=("$dir")
done < <(find_undeclared_dirs "$SCRIPT_DIR" "${DECLARED_TESTS[@]}")

if [[ ${#missing_from_manifest[@]} -gt 0 ]]; then
    echo "MANIFEST ERROR: found *_grate.c in these directories but run_tests.sh"
    echo "does not declare a test for them (add a run_test call, or an explicit"
    echo "skip_test with a reason, above):"
    printf '  - %s\n' "${missing_from_manifest[@]}"
    FAIL=$((FAIL + ${#missing_from_manifest[@]}))
fi

echo ""
echo "Results: $PASS passed, $FAIL failed, $SKIP skipped"
if [[ -n "${CATEGORY_COUNTS[*]+x}" ]]; then
    echo "Failure categories:"
    for cat in "${!CATEGORY_COUNTS[@]}"; do
        echo "  $cat: ${CATEGORY_COUNTS[$cat]}"
    done
fi
if [[ $FAIL -gt 0 ]]; then
    echo "Failed tests: ${FAILURES[*]}"
    exit 1
fi
if [[ $SKIP -gt 0 && "$ALLOW_SKIPS" != "yes" ]]; then
    echo "$SKIP maintained test(s) were skipped -- not a pass for gating/CI purposes."
    echo "Pass --allow-skips to accept this for an explicitly optional local run."
    exit 1
fi
