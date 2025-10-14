#!/bin/bash

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Array of test files - easily add more tests here
TEST_FILES=(
    # File tests (deterministic)
    "tests/unit-tests/file_tests/deterministic/readbytes.c"
    "tests/unit-tests/file_tests/deterministic/write.c"
    "tests/unit-tests/file_tests/deterministic/writeloop.c"
    "tests/unit-tests/file_tests/deterministic/writepartial.c"
    "tests/unit-tests/file_tests/deterministic/pread_pwrite.c"
    "tests/unit-tests/file_tests/deterministic/cloexec.c"
    "tests/unit-tests/file_tests/deterministic/filetest.c"
    "tests/unit-tests/file_tests/deterministic/creat_access.c"
    "tests/unit-tests/file_tests/deterministic/chmod.c"
    "tests/unit-tests/file_tests/deterministic/fchmod.c"
    "tests/unit-tests/file_tests/deterministic/fdatasync.c"
    "tests/unit-tests/file_tests/deterministic/fsync.c"
    "tests/unit-tests/file_tests/deterministic/ioctl.c"
    "tests/unit-tests/file_tests/deterministic/mkdir_rmdir.c"
    "tests/unit-tests/file_tests/deterministic/readlink.c"
    "tests/unit-tests/file_tests/deterministic/readlinkat.c"
    "tests/unit-tests/file_tests/deterministic/rename.c"
    "tests/unit-tests/file_tests/deterministic/stat.c"
    "tests/unit-tests/file_tests/deterministic/sync_file_range.c"
    "tests/unit-tests/file_tests/deterministic/truncate.c"
    "tests/unit-tests/file_tests/deterministic/unlinkat.c"

    # File tests (non-deterministic)
    "tests/unit-tests/file_tests/non-deterministic/chdir_getcwd.c"
    "tests/unit-tests/file_tests/non-deterministic/fchdir.c"
    "tests/unit-tests/file_tests/non-deterministic/fstatfs.c"
    "tests/unit-tests/file_tests/non-deterministic/getcwd.c"
    "tests/unit-tests/file_tests/non-deterministic/read.c"
    "tests/unit-tests/file_tests/non-deterministic/statfs.c"

    # Memory tests (deterministic)
    "tests/unit-tests/memory_tests/deterministic/mmap.c"
    "tests/unit-tests/memory_tests/deterministic/mmap_shared.c"
    "tests/unit-tests/memory_tests/deterministic/mmap_file.c"
    "tests/unit-tests/memory_tests/deterministic/mmap_complicated.c"
    "tests/unit-tests/memory_tests/deterministic/mmaptest.c"
    "tests/unit-tests/memory_tests/deterministic/mprotect.c"
    "tests/unit-tests/memory_tests/deterministic/sbrk.c"
    "tests/unit-tests/memory_tests/deterministic/shmtest.c"

    # Memory tests (non-deterministic)
    "tests/unit-tests/memory_tests/non-deterministic/shm.c"

    # Networking tests (deterministic)
    "tests/unit-tests/networking_tests/deterministic/poll.c"
    "tests/unit-tests/networking_tests/deterministic/recvfrom-sendto.c"
    "tests/unit-tests/networking_tests/deterministic/simple_epoll.c"
    "tests/unit-tests/networking_tests/deterministic/socket.c"
    "tests/unit-tests/networking_tests/deterministic/socketpair.c"
    "tests/unit-tests/networking_tests/deterministic/uds-getsockname.c"
    "tests/unit-tests/networking_tests/deterministic/uds-nb-select.c"

    # Networking tests (non-deterministic)
    "tests/unit-tests/networking_tests/non-deterministic/pipe.c"
    "tests/unit-tests/networking_tests/non-deterministic/simple-select.c"
    "tests/unit-tests/networking_tests/non-deterministic/socketepoll.c"
    "tests/unit-tests/networking_tests/non-deterministic/socketselect.c"

    # Signal/timer tests (deterministic)
    "tests/unit-tests/signal_tests/deterministic/setitimer.c"
)

echo "======================================"
echo "  Lind-WASM Build and Test Runner"
echo "======================================"
echo ""

# Run make all
echo "Running 'make all'..."
echo ""

if ! make all 2>&1 | tee build.log; then
    echo ""
    echo -e "${RED}Build failed!${NC}"
    echo ""
    echo "Error details:"
    tail -20 build.log
    rm -f build.log
    exit 1
fi

rm -f build.log

echo ""
echo -e "${GREEN}Build succeeded!${NC}"
echo ""
echo "======================================"
echo "  Running Tests"
echo "======================================"
echo ""

# Build the test command
TEST_CMD="python3 scripts/wasmtestreport.py --testfiles"
for test_file in "${TEST_FILES[@]}"; do
    TEST_CMD="$TEST_CMD $test_file"
done

# Run the tests and capture output
TEST_OUTPUT=$(eval $TEST_CMD 2>&1)
TEST_EXIT_CODE=$?

echo "$TEST_OUTPUT"
echo ""

# Parse results
echo "======================================"
echo "  Test Summary"
echo "======================================"
echo ""

PASSED=0
FAILED=0
TOTAL=${#TEST_FILES[@]}

# Parse the output for each test
for test_file in "${TEST_FILES[@]}"; do
    test_name=$(basename "$test_file" .c)
    # Check if this test has SUCCESS in the output
    if echo "$TEST_OUTPUT" | grep -q "$test_name.*SUCCESS"; then
        ((PASSED++))
        echo -e "${GREEN}✓${NC} $test_name: SUCCESS"
    else
        ((FAILED++))
        echo -e "${RED}✗${NC} $test_name: FAILED (no SUCCESS found)"
    fi
done

echo ""
echo "Total: $TOTAL tests"
echo -e "${GREEN}Passed: $PASSED${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
else
    echo -e "Failed: $FAILED"
fi
echo ""

exit $TEST_EXIT_CODE

