#!/usr/bin/env bash
# The general lind_extent_expr tree's evaluation ceilings (max depth, max
# node count) are independently hard-coded in two places, not derived from
# one shared definition -- see each site's own comment for why:
#   1. tools/marshal-gen/gen_grate.py            (LIND_EXTENT_EXPR_MAX_DEPTH/_MAX_NODES)
#   2. tests/grate-tests/lib-interpose/lind_marshal.h (same names, as #define)
#
# This script is the substitute for a real shared/generated constant,
# mirroring check_raw_arg_slot_consistency.sh's own pattern: it extracts the
# numeric value from each of the two sources and fails loudly if they
# disagree, rather than letting a partial edit silently desync generation-
# time validation from the runtime's own enforcement.
#
# Usage: tests/grate-tests/lib-interpose/check_extent_expr_bounds_consistency.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

fail=0

extract() {
    local label="$1" file="$2" pattern="$3"
    local val
    val="$(grep -oP "$pattern" "$file" | head -1)"
    if [[ -z "$val" ]]; then
        echo "MISSING: could not find $label's constant in $file"
        fail=1
        echo ""
        return
    fi
    echo "$val"
}

genrate_depth="$(extract "gen_grate.py:LIND_EXTENT_EXPR_MAX_DEPTH" \
    "$REPO_ROOT/tools/marshal-gen/gen_grate.py" \
    'LIND_EXTENT_EXPR_MAX_DEPTH\s*=\s*\K\d+')"
header_depth="$(extract "lind_marshal.h:LIND_EXTENT_EXPR_MAX_DEPTH" \
    "$SCRIPT_DIR/lind_marshal.h" \
    '#define\s+LIND_EXTENT_EXPR_MAX_DEPTH\s+\K\d+')"
genrate_nodes="$(extract "gen_grate.py:LIND_EXTENT_EXPR_MAX_NODES" \
    "$REPO_ROOT/tools/marshal-gen/gen_grate.py" \
    'LIND_EXTENT_EXPR_MAX_NODES\s*=\s*\K\d+')"
header_nodes="$(extract "lind_marshal.h:LIND_EXTENT_EXPR_MAX_NODES" \
    "$SCRIPT_DIR/lind_marshal.h" \
    '#define\s+LIND_EXTENT_EXPR_MAX_NODES\s+\K\d+')"

if [[ "$fail" -eq 1 ]]; then
    exit 1
fi

echo "gen_grate.py:LIND_EXTENT_EXPR_MAX_DEPTH  = $genrate_depth"
echo "lind_marshal.h:LIND_EXTENT_EXPR_MAX_DEPTH = $header_depth"
echo "gen_grate.py:LIND_EXTENT_EXPR_MAX_NODES  = $genrate_nodes"
echo "lind_marshal.h:LIND_EXTENT_EXPR_MAX_NODES = $header_nodes"

if [[ "$genrate_depth" == "$header_depth" && "$genrate_nodes" == "$header_nodes" ]]; then
    echo "ok    extent-expr depth/node-count ceilings agree (depth=$genrate_depth, nodes=$genrate_nodes)"
    exit 0
fi

echo "FAIL  extent-expr depth/node-count ceilings disagree across generation/runtime"
exit 1
