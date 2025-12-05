#!/usr/bin/env bash
set -euo pipefail

SYSROOT=/home/lind-wasm-rust/lind-wasm/src/glibc/sysroot
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
