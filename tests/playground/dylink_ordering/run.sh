#!/usr/bin/env bash
set -euo pipefail

R="${LIND_WASM_ROOT:-$HOME/lind-wasm}"
sudo LIND_WASM_ROOT="$R" "$R/scripts/bin/lind_run" --preload env=lib/lib.so /tests/dylink_ordering/main.cwasm
