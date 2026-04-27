#!/usr/bin/env bash
# End-to-end demo for remote library call delegation (scalar-only, Step 1).
#
# Prerequisites:
#   - lind-boot built with --features remote-lib
#   - lind-remote-server built (cargo build --manifest-path src/lind-boot/Cargo.toml --bin lind-remote-server)
#   - lind-clang on PATH
#   - gcc on PATH
#
# Run from the repo root: bash examples/remote-calls/run.sh

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

LIND_BOOT="$REPO_ROOT/src/lind-boot/target/debug/lind-boot"
LINDFS_ROOT="$REPO_ROOT/lindfs"
REMOTE_SERVER="$REPO_ROOT/src/lind-boot/target/debug/lind-remote-server"
SERVER_CFG="$SCRIPT_DIR/server_config.json"
ROUTING_CFG="$SCRIPT_DIR/routing_config.json"

# 1. Build the native shared library for the remote server
echo "==> Building native libtoy.so"
gcc -shared -fPIC -o $SCRIPT_DIR/libtoy.so "$SCRIPT_DIR/toy.c"

# 2. Build the WASM test application and toy WASM library
echo "==> Compiling test_app.c to WASM"
lind-clang "$SCRIPT_DIR/test_app.c"

echo "==> Compiling toy.c as a shared WASM library"
lind-clang --compile-library "$SCRIPT_DIR/toy.c"

# 3. Start the remote server in the background
echo "==> Starting remote server"
"$REMOTE_SERVER" "$SERVER_CFG" &
SERVER_PID=$!
trap "kill $SERVER_PID 2>/dev/null" EXIT
sleep 0.3   # give the server time to bind the socket

# 4. Run the WASM app with remote routing enabled
echo "==> Running WASM app (add/mul delegated to remote server)"
cp $ROUTING_CFG $LINDFS_ROOT/routing_config.json
LIND_REMOTE_CONFIG=routing_config.json \
    lind-wasm --preload env=toy.cwasm test_app.cwasm

echo "==> Done"
