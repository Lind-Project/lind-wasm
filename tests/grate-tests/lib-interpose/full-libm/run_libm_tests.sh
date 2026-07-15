#!/bin/bash
# Runs glibc's libm test suite (see gen_libm_tests.sh + compile_libm_tests.sh) either
# without interposition (baseline) or under a grate, by invoking
# run_single_libm_test.sh once per test, and reports a pass/fail summary.
#
# Usage:
#   ./run_libm_tests.sh                              # run all, no interposition
#   ./run_libm_tests.sh libm_full_grate.cwasm         # run all, under a grate
#   ./run_libm_tests.sh "" test-double-cos            # run one test, no interposition
#   ./run_libm_tests.sh libm_full_grate.cwasm test-double-cos   # one test, under a grate
#
# Results land in full-libm/generated/results[_<grate>]/<test>.result (written by
# run_single_libm_test.sh — see that script for the file format).
set -uo pipefail

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/libm_common.sh"

GRATE="${1:-}"
ONLY="${2:-}"
RESULTS="$HERE/generated/results${GRATE:+_${GRATE%.cwasm}}"

if [ -n "$GRATE" ] && [ ! -f "$REPO_ROOT/lindfs/grates/$GRATE" ]; then
  echo "error: grate not found: $REPO_ROOT/lindfs/grates/$GRATE (exact filename, including .cwasm)" >&2
  exit 2
fi

n=0; pass=0; fail=0
while read -r name rettype; do
  n=$((n+1))
  "$HERE/run_single_libm_test.sh" "$name" "$GRATE" >/dev/null
  if grep -q "All tests passed successfully\|errors\? occurred" "$RESULTS/$name.result" 2>/dev/null; then
    grep -q "All tests passed successfully" "$RESULTS/$name.result" && pass=$((pass+1)) || fail=$((fail+1))
  else
    fail=$((fail+1))
  fi
  [ $((n % 25)) -eq 0 ] && echo "progress: $n"
done < <(worklist "$ONLY")

echo ""
echo "=== done: $n run (mode: ${GRATE:-no interposition}) ==="
echo "results in: $RESULTS"
echo "(pass/fail above is a rough per-binary signal — the real classification below"
echo " distinguishes numerically-correct-but-flag-only failures from genuine bugs/crashes,"
echo " since a 'fail' binary can still be numerically correct if only FP-exception-flag or"
echo " errno checks differ — see classify_libm_results.py and ../LIBM_INTERPOSITION.md)"
echo ""
python3 "$HERE/classify_libm_results.py" "$RESULTS"
