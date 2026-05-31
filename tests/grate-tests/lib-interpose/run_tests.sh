#!/usr/bin/env bash
# Run all lib-interpose grate tests and report pass/fail.
#
# Usage (from any directory):
#   bash tests/grate-tests/lib-interpose/run_tests.sh
#
# Each test runs inside lindfs/ (the lind filesystem root).
# Cage wasm binaries that are not already in lindfs are staged to a temporary
# location inside lindfs for the duration of the test and cleaned up afterward.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LINDFS="$REPO_ROOT/lindfs"
GRATES_DIR="$LINDFS/grates"

PASS=0
FAIL=0
ERRORS=()

# Stage a file into lindfs for the duration of one test, then clean it up.
# Usage: run_test <test_name> <staged_src> <staged_dst_in_lindfs> <lind-wasm args...>
#   staged_src=""  and staged_dst=""  → no staging needed (e.g. zlib-python)
run_test() {
    local name="$1"
    local staged_src="$2"
    local staged_dst="$3"
    shift 3
    # remaining args are passed verbatim to lind-wasm

    local staged_abs=""
    if [[ -n "$staged_src" ]]; then
        staged_abs="$LINDFS/$staged_dst"
        cp "$staged_src" "$staged_abs"
    fi

    local output exit_code
    output=$(cd "$LINDFS" && lind-wasm "$@" 2>&1) || true
    exit_code=$?

    if [[ -n "$staged_abs" ]]; then
        rm -f "$staged_abs"
    fi

    # A test passes when lind-wasm exits 0 AND the output contains "PASS".
    if [[ $exit_code -eq 0 ]] && echo "$output" | grep -q "PASS"; then
        echo "  PASS  $name"
        PASS=$((PASS + 1))
    else
        echo "  FAIL  $name (exit=$exit_code)"
        # Print the last few lines of output to help diagnose failures.
        echo "$output" | tail -10 | sed 's/^/         /'
        FAIL=$((FAIL + 1))
        ERRORS+=("$name")
    fi
}

echo "=== lib-interpose tests ==="
echo ""

# libc-rand: intercepts rand() and returns a fixed value
run_test "libc-rand" \
    "$SCRIPT_DIR/libc-rand/libc-rand.cwasm" \
    "libc-rand.cwasm" \
    grates/libc-rand_grate.cwasm /libc-rand.cwasm

# libc-strlen: intercepts strlen() and returns len*2
run_test "libc-strlen" \
    "$SCRIPT_DIR/libc-strlen/libc-strlen.cwasm" \
    "libc-strlen.cwasm" \
    grates/libc-strlen_grate.cwasm /libc-strlen.cwasm

# custom-lib: intercepts toy_add and toy_mul from a preloaded wasm library
run_test "custom-lib" \
    "$SCRIPT_DIR/custom-lib/custom-lib.cwasm" \
    "custom-lib.cwasm" \
    --preload env=/lib/libtoy.cwasm \
    grates/custom-lib_grate.cwasm /custom-lib.cwasm

# zlib-python: intercepts deflate() so Python's zlib.compress() returns b"LIND"
# No staging needed: the grate and Python binary are already in lindfs.
run_test "zlib-python" \
    "" "" \
    --preload env=/lib/libz.so \
    --preload env=/lib/libpython3.14.so \
    grates/zlib-python_grate.cwasm \
    /usr/local/bin/python /test-zlib.py

# --- Stage-1 automated marshalling tests ---

# auto-scalar: intercepts toy_add with SCALAR spec; handler returns a*b
run_test "auto-scalar" \
    "$SCRIPT_DIR/auto-scalar/auto-scalar.cwasm" \
    "auto-scalar.cwasm" \
    --preload env=/lib/libtoy.cwasm \
    grates/auto-scalar_grate.cwasm /auto-scalar.cwasm

# auto-memcpy: intercepts memcpy with PTR IN/OUT spec + return alias
run_test "auto-memcpy" \
    "$SCRIPT_DIR/auto-memcpy/auto-memcpy.cwasm" \
    "auto-memcpy.cwasm" \
    grates/auto-memcpy_grate.cwasm /auto-memcpy.cwasm

# auto-strncpy: intercepts strncpy with PTR IN/OUT spec + return alias
run_test "auto-strncpy" \
    "$SCRIPT_DIR/auto-strncpy/auto-strncpy.cwasm" \
    "auto-strncpy.cwasm" \
    grates/auto-strncpy_grate.cwasm /auto-strncpy.cwasm

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [[ $FAIL -gt 0 ]]; then
    echo "Failed tests: ${ERRORS[*]}"
    exit 1
fi
