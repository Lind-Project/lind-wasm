#!/bin/bash
# Build and run the strcpy remote-call example with a custom server-side handler.
# Run from the repo root: bash examples/remote-calls-ptr/run_strcpy.sh

set -e
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
EXAMPLE="$REPO/examples/remote-calls-ptr"
LINDFS="$REPO/lindfs"

# 1. Compile the custom server-side handler to a native shared library.
echo "==> Building custom handler shared library..."
CUSTOM_SO="$EXAMPLE/custom_handlers.so"
gcc -shared -fPIC -o "$CUSTOM_SO" "$EXAMPLE/custom_handlers.c"

# 2. Generate the server config with the real .so path substituted in.
REAL_CONFIG="$EXAMPLE/server_config_strcpy_real.json"
sed "s|CUSTOM_HANDLERS_SO_PATH|$CUSTOM_SO|g" \
    "$EXAMPLE/server_config_strcpy.json" > "$REAL_CONFIG"

# 3. Build the WASM binary (-fno-builtin-strcpy prevents the compiler from
#    substituting an inlined builtin in place of the library import).
echo "==> Building WASM binary..."
lind-clang "$EXAMPLE/test_strcpy.c" -- -fno-builtin-strcpy
WASM="test_strcpy.cwasm"

# 4. Path to the remote server binary.
SERVER="$REPO/src/lind-boot/target/release/lind-remote-server"

# 5. Start the server in the background.
echo "==> Starting server..."
"$SERVER" "$REAL_CONFIG" &
SERVER_PID=$!
sleep 0.3

# 6. Run the Lind cage with the routing config.
echo "==> Running cage..."
cp "$EXAMPLE/routing_strcpy.json" "$LINDFS/routing.json"
LIND_REMOTE_CONFIG="routing.json" \
    lind-wasm "$WASM"

# 7. Clean up.
kill "$SERVER_PID" 2>/dev/null || true
rm -f "$REAL_CONFIG"
