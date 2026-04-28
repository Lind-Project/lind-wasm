#!/bin/bash
# Per-cage routing example: three cages call strcpy with different policies.
#
#   cage 1 — local call, no interposition
#   cage 2 — routed to a Unix domain socket server
#   cage 3 — routed to a TCP socket server
#
# Usage (from repo root):
#   bash examples/remote-calls-percage/run.sh

set -e

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
EXAMPLE="$REPO/examples/remote-calls-percage"
LINDFS="$REPO/lindfs"
SERVER="$REPO/src/lind-boot/target/release/lind-remote-server"

# ---- 1. Build the native handler shared library ----
echo "==> Building remote_strcpy.so..."
SO="$EXAMPLE/remote_strcpy.so"
gcc -shared -fPIC -o "$SO" "$EXAMPLE/remote_strcpy.c"

# ---- 2. Build the WASM binary ----
# -fno-builtin-strcpy prevents the compiler from inlining strcpy so the call
# goes through glibc and can be intercepted by the routing layer.
echo "==> Building WASM binary..."
lind-clang "$EXAMPLE/test_percage.c" -- -fno-builtin-strcpy
WASM="test_percage.cwasm"

# ---- 3. Generate concrete server configs ----
# Unix server: socket lives under lindfs/tmp so the chroot-side client path
# (/tmp/percage_unix.sock) resolves correctly.
mkdir -p "$LINDFS/tmp"
SOCK_PATH="$LINDFS/tmp/percage_unix.sock"

UNIX_CFG="$EXAMPLE/server_unix_real.json"
sed -e "s|SO_PATH|$SO|g" -e "s|SOCK_PATH|$SOCK_PATH|g" \
    "$EXAMPLE/server_unix.json" > "$UNIX_CFG"

# TCP server: endpoint is a plain address, only the library path needs substitution.
TCP_CFG="$EXAMPLE/server_tcp_real.json"
sed "s|SO_PATH|$SO|g" "$EXAMPLE/server_tcp.json" > "$TCP_CFG"

# ---- 4. Start both servers ----
echo "==> Starting Unix socket server..."
"$SERVER" "$UNIX_CFG" &
UNIX_PID=$!

echo "==> Starting TCP socket server (127.0.0.1:19000)..."
"$SERVER" "$TCP_CFG" &
TCP_PID=$!

# Give both servers time to bind before the cage connects.
sleep 0.5

# ---- 5. Run the cage with the per-cage routing config ----
echo "==> Running Lind cage..."
cp "$EXAMPLE/routing.json" "$LINDFS/routing.json"
LIND_REMOTE_CONFIG="routing.json" lind-wasm "$WASM"

# ---- 6. Clean up ----
kill "$UNIX_PID" "$TCP_PID" 2>/dev/null || true
rm -f "$UNIX_CFG" "$TCP_CFG"
