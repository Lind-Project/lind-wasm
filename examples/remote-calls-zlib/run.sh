#!/bin/bash
# Python zlib interposition example: crc32 and adler32 are delegated to a
# Unix domain socket server running the native zlib implementation.
#
# Usage (from repo root):
#   bash examples/remote-calls-zlib/run.sh

set -e

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
EXAMPLE="$REPO/examples/remote-calls-zlib"
LINDFS="$REPO/lindfs"
SERVER="$REPO/src/lind-boot/target/release/lind-remote-server"

# ---- 1. Build the native handler shared library ----
echo "==> Building remote_zlib.so..."
SO="$EXAMPLE/remote_zlib.so"
gcc -shared -fPIC -o "$SO" "$EXAMPLE/remote_zlib.c"

# ---- 2. Generate concrete server config ----
# Socket lives under lindfs/tmp so the chroot-side client path
# (/tmp/zlib_remote.sock) resolves correctly.
mkdir -p "$LINDFS/tmp"
SOCK_PATH="$LINDFS/tmp/zlib_remote.sock"

REAL_CFG="$EXAMPLE/server_real.json"
sed -e "s|SO_PATH|$SO|g" -e "s|SOCK_PATH|$SOCK_PATH|g" \
    "$EXAMPLE/server.json" > "$REAL_CFG"

# ---- 3. Start the server ----
echo "==> Starting Unix socket server..."
"$SERVER" "$REAL_CFG" &
SERVER_PID=$!

# Give the server time to bind before the cage connects.
sleep 0.5

# ---- 4. Run Python with the routing config ----
echo "==> Running Python zlib test..."
cp "$EXAMPLE/routing.json" "$LINDFS/routing.json"
LIND_REMOTE_CONFIG="routing.json" lind-wasm \
    --preload env=lib/libz.so \
    --preload env=lib/libpython3.14.so \
    bin/python zlib.py

# ---- 5. Clean up ----
kill "$SERVER_PID" 2>/dev/null || true
# rm -f "$REAL_CFG"
