#!/usr/bin/env bash
# Run the same Clippy invocation as .github/workflows/lint.yml (see docs/contribute/clippy.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LIND_BOOT_FEATURES="${LIND_BOOT_FEATURES:-disable_signals secure lind_debug lind-logging debug-grate-calls fdtables-dashmaparray}"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-nightly-2025-06-08}"

exec cargo "+${RUST_TOOLCHAIN}" clippy \
  --manifest-path src/lind-boot/Cargo.toml \
  --features "${LIND_BOOT_FEATURES}" \
  --keep-going \
  --all-targets \
  -- \
  -A warnings \
  -A clippy::not_unsafe_ptr_arg_deref
