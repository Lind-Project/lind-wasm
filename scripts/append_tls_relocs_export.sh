#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   ./append_tls_relocs_export.sh input.wasm output.wasm
#
# Requires:
#   wabt tools: wasm2wat, wat2wasm

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 input.wasm output.wasm" >&2
  exit 1
fi

IN_WASM="$1"
OUT_WASM="$2"

if [[ ! -f "$IN_WASM" ]]; then
  echo "Error: input wasm not found: $IN_WASM" >&2
  exit 1
fi

if ! command -v wasm2wat >/dev/null 2>&1 || ! command -v wat2wasm >/dev/null 2>&1; then
  echo "Error: wasm2wat and/or wat2wasm not found in PATH (install wabt)." >&2
  exit 1
fi

INSERT_LINE="  (export \"__wasm_apply_tls_relocs\" (func \$__wasm_apply_tls_relocs))"
ANCHOR_LINE="  (export \"__wasm_apply_data_relocs\" (func \$__wasm_apply_data_relocs))"

tmpdir="$(mktemp -d)"
cleanup() { rm -rf "$tmpdir"; }
trap cleanup EXIT

wat_in="$tmpdir/in.wat"
wat_out="$tmpdir/out.wat"

# 1) Convert wasm -> wat
/home/lind/wabt-1.0.37/bin/wasm2wat --enable-all "$IN_WASM" -o "$wat_in"

# 2) If export already present, keep as-is
if grep -Fqx "$INSERT_LINE" "$wat_in"; then
  echo "Export \"__wasm_apply_tls_relocs\" already exists, skip"
  exit 0
else
  # Ensure anchor exists
  if ! grep -Fqx "$ANCHOR_LINE" "$wat_in"; then
    echo "Error: anchor export line not found in WAT:" >&2
    echo "  $ANCHOR_LINE" >&2
    exit 2
  fi

  # Insert after the anchor (first occurrence)
  awk -v anchor="$ANCHOR_LINE" -v ins="$INSERT_LINE" '
    BEGIN { inserted=0 }
    {
      print
      if (!inserted && $0 == anchor) {
        print ins
        inserted=1
      }
    }
    END {
      if (!inserted) {
        # Should not happen because we pre-checked with grep, but keep it safe
        exit 3
      }
    }
  ' "$wat_in" > "$wat_out"
fi

# 3) Convert wat -> wasm
/home/lind/wabt-1.0.37/bin/wat2wasm --enable-all "$wat_out" -o "$OUT_WASM"

echo "Patched wasm written to: $OUT_WASM"