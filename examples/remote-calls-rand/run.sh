#!/usr/bin/env bash
# Test: interpose glibc rand() and delegate it to a remote server.
#
# rand() is a reliable interposition target: it has internal state so the
# compiler never inlines or constant-folds it, guaranteeing a real function
# call that our wrapper can intercept.
#
# The remote server's rand() always returns 42424242 (sentinel). Seeing that
# value instead of pseudo-random output proves the call went over the RPC.
#
# Run from the repo root: bash examples/remote-calls-rand/run.sh

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

LINDFS_ROOT="$REPO_ROOT/lindfs"
REMOTE_SERVER="$REPO_ROOT/src/lind-boot/target/debug/lind-remote-server"
SERVER_CFG="$SCRIPT_DIR/server_config.json"
ROUTING_CFG="$SCRIPT_DIR/routing_config.json"

# 1. Build the native sentinel library for the remote server
echo "==> Building native librand.so"
gcc -shared -fPIC -o "$SCRIPT_DIR/librand.so" "$SCRIPT_DIR/rand_lib.c"

# 2. Compile the WASM test app (glibc provides rand automatically — no --preload needed)
echo "==> Compiling test_rand.c to WASM"
lind-clang "$SCRIPT_DIR/test_rand.c"

# 3. Start the remote server
echo "==> Starting remote server"
"$REMOTE_SERVER" "$SERVER_CFG" &
SERVER_PID=$!
trap "kill $SERVER_PID 2>/dev/null" EXIT
sleep 0.3

# 4. Run with remote routing — rand() calls go to the server
echo "==> Running WASM app (rand delegated to remote; expect 42424242)"
cp "$ROUTING_CFG" "$LINDFS_ROOT/routing_config.json"
LIND_REMOTE_CONFIG=routing_config.json \
    lind-wasm test_rand.cwasm

echo "==> Done"
