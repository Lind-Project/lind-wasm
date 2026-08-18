#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

R="${LIND_WASM_ROOT:-$HOME/lind-wasm}"
LC="$R/scripts/bin/lind_compile"

"$LC" --compile-library --output-dir lib lib.c
# -rdynamic: export main's own symbols (xerbla_ included) so a preloaded
# library's call into main can resolve against them. Without this, wasm-ld's
# default main-executable link doesn't export ordinary symbols at all -- a
# separate wasm-ld gap (issue #2) we need to route around to isolate the
# load-order issue (issue #1) this reproducer targets.
"$LC" -rdynamic --output-dir tests/dylink_ordering main.c
