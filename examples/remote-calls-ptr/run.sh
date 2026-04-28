#!/bin/bash
# Build and run the ptr-argument remote-call example (strlen interposition).
# Run from the repo root: bash examples/remote-calls-ptr/run.sh

set -e
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
EXAMPLE="$REPO/examples/remote-calls-ptr"
LINDFS="$REPO/lindfs"

# 1. Build the WASM binary for the Lind cage.
echo "==> Building WASM binary..."
lind-clang "$EXAMPLE/test_ptr.c"
# lind-clang always places the output at the lindfs root.
WASM="test_ptr.cwasm"

# 2. Build the remote server (requires remote-lib feature).
SERVER="$REPO/src/lind-boot/target/release/lind-remote-server"

# 3. Start the server in the background.
echo "==> Starting server (system libc strlen)..."
"$SERVER" "$EXAMPLE/server_config.json" &
SERVER_PID=$!
sleep 0.3

# 4. Run the Lind cage with the routing config.
echo "==> Running cage..."
cp $EXAMPLE/routing.json $LINDFS/routing.json
LIND_REMOTE_CONFIG="routing.json" \
    lind-wasm "$WASM"

# 5. Clean up.
kill "$SERVER_PID" 2>/dev/null || true
