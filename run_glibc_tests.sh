#!/bin/bash

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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
echo "  Running Tests"
echo "======================================"
echo ""

PASSED=0
FAILED=0
TOTAL=${#TEST_FILES[@]}

# Run each test individually and show output
for i in "${!TEST_FILES[@]}"; do
    test_file="${TEST_FILES[$i]}"
    test_name=$(basename "$test_file" .c)
    test_num=$((i + 1))
    
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Test [$test_num/$TOTAL]: $test_name${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    
    # Run single test with wasmtestreport.py
    if python3 scripts/wasmtestreport.py --testfiles "$test_file" 2>&1 | tee test_output.log; then
        # Check if the test actually succeeded by looking at the output
        if grep -q "\[INFO\] SUCCESS" test_output.log; then
            ((PASSED++))
            echo -e "${GREEN}✓ PASSED${NC}"
        else
            ((FAILED++))
            echo -e "${RED}✗ FAILED${NC}"
        fi
    else
        ((FAILED++))
        echo -e "${RED}✗ FAILED (execution error)${NC}"
    fi
    
    rm -f test_output.log
done

# Final Summary
echo ""
echo ""
echo "======================================"
echo "  Test Summary"
echo "======================================"
echo ""
echo "Total: $TOTAL tests"
echo -e "${GREEN}Passed: $PASSED${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
    echo ""
    exit 1
else
    echo -e "Failed: $FAILED"
    echo ""
    exit 0
fi

