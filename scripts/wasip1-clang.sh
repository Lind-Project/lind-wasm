#!/usr/bin/env bash
set -euo pipefail

# --- repo root discovery (env var -> script dir -> git) ---
if [[ -n "${LIND_WASM_ROOT:-}" && -d "${LIND_WASM_ROOT}" ]]; then
  REPO_ROOT="${LIND_WASM_ROOT}"
else
  SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
  if [[ -f "${SCRIPT_DIR}/../Makefile" ]]; then
    REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
  else
    if command -v git >/dev/null 2>&1; then
      REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
    else
      REPO_ROOT=""
    fi
  fi
fi

if [[ -z "${REPO_ROOT}" || ! -d "${REPO_ROOT}" ]]; then
  echo "ERROR: Could not locate lind-wasm repo root." >&2
  echo "Hint: export LIND_WASM_ROOT=/path/to/lind-wasm" >&2
  exit 2
fi

SYSROOT="$REPO_ROOT/src/glibc/sysroot"
LIBDIR="$SYSROOT/lib/wasm32-wasi"
CRT1="$LIBDIR/crt1.o"

# Sanity checks (fail early with a clear message)
[ -r "$CRT1" ] || { echo "Missing $CRT1"; exit 1; }
[ -d "$LIBDIR" ] || { echo "Missing $LIBDIR"; exit 1; }

# Build the actual clang command we will exec
cmd=(
  clang
  --target=wasm32-unknown-wasip1
  --sysroot="$SYSROOT"
  -nostartfiles          # prevent clang from looking for its own crt1.o
)

# Forward all rustc-provided args
cmd+=("$@")

# Inject our startup object and libraries **after** user objects
cmd+=(
  "$CRT1"
  -L"$LIBDIR"
  -lc
  -pthread
)

# Show the exact command we will run (to STDERR so rustc keeps its stdout clean)
echo "[clang wrapper exec]" "${cmd[@]}" 1>&2

# Run it
exec "${cmd[@]}"

