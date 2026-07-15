#!/bin/bash
# Runs one already-compiled libm test wasm (see compile_libm_tests.sh), either without
# interposition (baseline) or under a grate, and writes a result file.
#
# Usage:
#   ./run_single_libm_test.sh <test-name>                    # no interposition
#   ./run_single_libm_test.sh <test-name> libm_full_grate.cwasm   # under a grate
#
# Grate name is resolved as lindfs/grates/<name> (matching `scripts/lind_run <grate>`).
# Result file: generated/results[_<grate>]/<test>.result — first line `RC=<n>`, rest is
# the test's own stdout+stderr (its own pass/fail summary is embedded). Exits with the
# test binary's own exit code (or 2 if it wasn't compiled yet).
set -uo pipefail

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/libm_common.sh"

NAME="${1:?usage: $0 <test-name> [grate.cwasm]}"
GRATE="${2:-}"
RESULTS="$HERE/generated/results${GRATE:+_${GRATE%.cwasm}}"
mkdir -p "$RESULTS"

wasm="$OBJ/$NAME.wasm"
if [ ! -f "$wasm" ]; then
  echo "NOT_COMPILED" > "$RESULTS/$NAME.result"
  echo "error: $wasm missing — run ./compile_libm_tests.sh $NAME first" >&2
  exit 2
fi

if [ -n "$GRATE" ] && [ ! -f "$REPO_ROOT/lindfs/grates/$GRATE" ]; then
  echo "GRATE_NOT_FOUND" > "$RESULTS/$NAME.result"
  echo "error: grate not found: $REPO_ROOT/lindfs/grates/$GRATE (exact filename, including .cwasm)" >&2
  exit 2
fi

cd "$REPO_ROOT"
chmod u+rwx lindfs 2>/dev/null
cp "$wasm" "lindfs/$NAME.wasm"
out=""; rc=0
if [ -n "$GRATE" ]; then
  out=$(timeout 30 scripts/lind_run "grates/$GRATE" "$NAME.wasm" 2>&1); rc=$?
else
  out=$(timeout 30 scripts/lind_run "$NAME.wasm" 2>&1); rc=$?
fi
rm -f "lindfs/$NAME.wasm"
{ echo "RC=$rc"; echo "$out"; } > "$RESULTS/$NAME.result"

cat "$RESULTS/$NAME.result"
exit "$rc"
