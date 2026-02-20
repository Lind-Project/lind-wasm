#!/bin/bash
# Test all network lmbench benchmarks to confirm they all hit the same crash.
# Run inside the Docker container after: make lind-debug
#
# Usage: bash test_network_benchmarks.sh 2>&1 | tee network_crash_report.log

set -e

LIND_RUN="/home/lind/lind-wasm/scripts/lind_run"
WASM_DIR="/lmbench/wasm32-wasi"
LOG_SEP="================================================================"

echo "$LOG_SEP"
echo "Network lmbench crash reproduction — $(date)"
echo "$LOG_SEP"

# Helper: run a benchmark and capture exit code + stderr
run_test() {
    local label="$1"
    local server_cmd="$2"
    local client_cmd="$3"

    echo ""
    echo "$LOG_SEP"
    echo "TEST: $label"
    echo "$LOG_SEP"

    if [ -n "$server_cmd" ]; then
        echo "--- Starting server: $server_cmd ---"
        # Run server via bash background so daemon pattern works
        $LIND_RUN /bin/bash -c "$server_cmd & /bin/msleep 2000; echo SERVER_LAUNCHED" 2>&1 &
        local server_pid=$!
        sleep 4  # wait for server to bind
        echo "--- Server launched (host pid=$server_pid) ---"
    fi

    echo "--- Running client: $client_cmd ---"
    set +e
    $LIND_RUN $client_cmd 2>&1
    local rc=$?
    set -e
    echo "--- Client exit code: $rc ---"

    if [ -n "$server_pid" ]; then
        # Try to kill server background
        kill $server_pid 2>/dev/null || true
        wait $server_pid 2>/dev/null || true
    fi

    return 0
}

# ---- Test 1: lat_tcp (baseline — known crash) ----
run_test "lat_tcp client (direct, no server)" \
    "" \
    "$WASM_DIR/lat_tcp.opt.wasm 127.0.0.1"

# ---- Test 2: lat_tcp with server running ----
run_test "lat_tcp client (with server via bash)" \
    "$WASM_DIR/lat_tcp.opt.wasm -s" \
    "$WASM_DIR/lat_tcp.opt.wasm 127.0.0.1"

# ---- Test 3: lat_udp ----
run_test "lat_udp client (direct, no server)" \
    "" \
    "$WASM_DIR/lat_udp.opt.wasm 127.0.0.1"

# ---- Test 4: lat_udp with server running ----
run_test "lat_udp client (with server via bash)" \
    "$WASM_DIR/lat_udp.opt.wasm -s" \
    "$WASM_DIR/lat_udp.opt.wasm 127.0.0.1"

# ---- Test 5: lat_connect ----
run_test "lat_connect client (direct, no server)" \
    "" \
    "$WASM_DIR/lat_connect.opt.wasm 127.0.0.1"

# ---- Test 6: lat_connect with server running ----
run_test "lat_connect client (with server via bash)" \
    "$WASM_DIR/lat_tcp.opt.wasm -s" \
    "$WASM_DIR/lat_connect.opt.wasm 127.0.0.1"

# ---- Test 7: bw_tcp ----
run_test "bw_tcp client (direct, no server)" \
    "" \
    "$WASM_DIR/bw_tcp.opt.wasm 127.0.0.1"

# ---- Test 8: bw_tcp with server running ----
run_test "bw_tcp client (with server via bash)" \
    "$WASM_DIR/bw_tcp.opt.wasm -s" \
    "$WASM_DIR/bw_tcp.opt.wasm 127.0.0.1"

echo ""
echo "$LOG_SEP"
echo "ALL TESTS COMPLETE"
echo "$LOG_SEP"
