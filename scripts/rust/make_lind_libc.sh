#!/usr/bin/env bash
# Build a lind-compatible copy of the Rust `libc` crate.
#
# The upstream crate's `wasi` module follows wasi-libc. lind-glibc is 32-bit
# Linux, so struct layouts, O_* flags and errno values differ and Rust std's
# fs metadata, read_dir and File::create break. This script copies
# libc-$LIBC_VER from the cargo registry to OUT_DIR and applies
# lind-libc-wasi.patch.
#
# Usage: make_lind_libc.sh [OUT_DIR] [--patch-std <toolchain>] [--unpatch-std <toolchain>]
#   OUT_DIR default: $LIND_WASM_ROOT/build/lind-libc
#
# Build a crate with:
#   cargo +<toolchain> --config "patch.crates-io.libc.path=\"OUT_DIR\"" \
#       build -Z build-std=std,panic_abort --target wasm32-wasip1
#
# --patch-std: -Z build-std ignores the user [patch] table and takes std's
# deps from the rust-src copy, so we edit that copy in place (originals kept
# as *.lind-orig): library/Cargo.toml [patch], library/Cargo.lock libc entry,
# and two `as u64` casts in std/src/os/wasi/fs.rs.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${LIND_WASM_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
LIBC_VER="${LIBC_VER:-0.2.189}"
OUT=""; PATCH_STD=""; UNPATCH_STD=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --patch-std) PATCH_STD="$2"; shift ;;
    --unpatch-std) UNPATCH_STD="$2"; shift ;;
    -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
    *) OUT="$1" ;;
  esac
  shift
done
OUT="${OUT:-$REPO_ROOT/build/lind-libc}"
OUT="$(mkdir -p "$OUT" && cd "$OUT" && pwd)"
PATCH="$SCRIPT_DIR/lind-libc-wasi.patch"
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"

log() { echo "[lind-libc] $*" >&2; }

std_lib_dir() {
  local sysroot; sysroot="$(rustc "+$1" --print sysroot)"
  echo "$sysroot/lib/rustlib/src/rust/library"
}

unpatch_std() {
  local lib; lib="$(std_lib_dir "$1")"
  for f in Cargo.toml Cargo.lock std/src/os/wasi/fs.rs; do
    [[ -f "$lib/$f.lind-orig" ]] && cp "$lib/$f.lind-orig" "$lib/$f" && log "restored $f"
  done
}

patch_std() {
  local lib; lib="$(std_lib_dir "$1")"
  [[ -f "$lib/Cargo.toml" ]] || { log "ERROR: rust-src not installed for $1 (rustup component add rust-src --toolchain $1)"; exit 1; }
  for f in Cargo.toml Cargo.lock std/src/os/wasi/fs.rs; do
    [[ -f "$lib/$f.lind-orig" ]] || cp "$lib/$f" "$lib/$f.lind-orig"
  done
  # 1. Cargo.toml
  if grep -q "# lind-wasm overlay" "$lib/Cargo.toml"; then
    sed -i "s#^libc = { path = \".*\" } \# lind-wasm overlay#libc = { path = \"$OUT\" } \# lind-wasm overlay#" "$lib/Cargo.toml"
  else
    sed -i "0,/^\[patch.crates-io\]/s##[patch.crates-io]\nlibc = { path = \"$OUT\" } \# lind-wasm overlay#" "$lib/Cargo.toml"
  fi
  grep -q "# lind-wasm overlay" "$lib/Cargo.toml" || { log "ERROR: failed to patch $lib/Cargo.toml"; exit 1; }
  # 2. Cargo.lock
  python3 - "$lib/Cargo.lock" "$LIBC_VER" <<'PY'
import re, sys
p, ver = sys.argv[1], sys.argv[2]
s = open(p).read()
m = re.search(r'\[\[package\]\]\nname = "libc"\nversion = "([^"]+)"\n(source = "[^"]+"\n)?(checksum = "[^"]+"\n)?', s)
assert m, "libc entry not found in std Cargo.lock"
s = s.replace(m.group(0), '[[package]]\nname = "libc"\nversion = "%s"\n' % ver)
open(p, 'w').write(s)
PY
  # 3. os/wasi/fs.rs
  sed -i -e 's/self\.as_inner()\.as_inner()\.st_ino$/self.as_inner().as_inner().st_ino as u64 \/\/ lind-wasm overlay/' \
         -e 's/self\.as_inner()\.as_inner()\.st_nlink$/self.as_inner().as_inner().st_nlink as u64 \/\/ lind-wasm overlay/' \
         "$lib/std/src/os/wasi/fs.rs"
  log "std workspace of $1 patched to use $OUT ($lib)"
}

if [[ -n "$UNPATCH_STD" ]]; then unpatch_std "$UNPATCH_STD"; exit 0; fi

STAMP="$OUT/.lind-libc-stamp"
WANT="$LIBC_VER $(sha256sum "$PATCH" | cut -c1-16)"
if [[ -f "$STAMP" && "$(cat "$STAMP")" == "$WANT" ]]; then
  log "up to date at $OUT (libc $LIBC_VER)"
  [[ -n "$PATCH_STD" ]] && patch_std "$PATCH_STD"
  exit 0
fi

find_src() { ls -d "$CARGO_HOME_DIR"/registry/src/*/libc-"$LIBC_VER" 2>/dev/null | head -1; }
SRC="$(find_src || true)"
if [[ -z "$SRC" ]]; then
  log "libc $LIBC_VER not in registry; fetching"
  TMP="$(mktemp -d)"
  ( cd "$TMP" && cargo init -q --name lind-libc-fetch --lib >/dev/null 2>&1 \
      && cargo add -q "libc@=$LIBC_VER" >/dev/null 2>&1 && cargo fetch -q )
  rm -rf "$TMP"
  SRC="$(find_src || true)"
fi
[[ -n "$SRC" ]] || { log "ERROR: could not locate libc-$LIBC_VER under $CARGO_HOME_DIR/registry/src"; exit 1; }

log "copying $SRC -> $OUT"
rm -rf "$OUT"; mkdir -p "$(dirname "$OUT")"
cp -R "$SRC" "$OUT"
rm -f "$OUT/Cargo.toml.orig" "$OUT/.cargo_vcs_info.json" "$OUT/.cargo-ok"
log "applying $(basename "$PATCH")"
patch -p1 -d "$OUT" --forward --silent < <(sed -e 's#^--- libc-orig/#--- a/#' -e 's#^+++ libc-lind/#+++ b/#' "$PATCH")
echo "$WANT" > "$STAMP"
log "done: $OUT"
[[ -n "$PATCH_STD" ]] && patch_std "$PATCH_STD"
exit 0
