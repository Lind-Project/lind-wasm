// Comprehensive test for mmap and shared memory error handling
// This test verifies error codes are properly propagated for memory operations
// Issue #451: Ensure libc::mmap return values are correctly distinguished from errors

#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/shm.h>
#include <errno.h>
#include <string.h>
#include <unistd.h>
#include <assert.h>

int main() {
    void *result;

    printf("=== Memory Error Handling Test (mmap + shmat) ===\n\n");

    // =====================
    // MMAP ERROR TESTS
    // =====================

    // Test 1: mmap with invalid file descriptor
    printf("Test 1: mmap with invalid file descriptor (should fail)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, 999, 0);
    assert(result == MAP_FAILED);
    printf("PASSED\n");

    // Test 2: mmap with unaligned address (MAP_FIXED)
    printf("Test 2: mmap with unaligned address (should fail with EINVAL)... ");
    errno = 0;
    result = mmap((void *)0x1001, 4096, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    assert(result == MAP_FAILED);
    assert(errno == EINVAL);
    printf("PASSED\n");

    // Test 3: mmap with unaligned offset
    printf("Test 3: mmap with unaligned offset (should fail with EINVAL)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS, -1, 123);
    assert(result == MAP_FAILED);
    assert(errno == EINVAL);
    printf("PASSED\n");

    // Test 4: mmap with both MAP_PRIVATE and MAP_SHARED
    printf("Test 4: mmap with both MAP_PRIVATE and MAP_SHARED (should fail with EINVAL)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE | MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    assert(result == MAP_FAILED);
    assert(errno == EINVAL);
    printf("PASSED\n");

    // Test 5: mmap with neither MAP_PRIVATE nor MAP_SHARED
    printf("Test 5: mmap with neither MAP_PRIVATE nor MAP_SHARED (should fail with EINVAL)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_ANONYMOUS, -1, 0);
    assert(result == MAP_FAILED);
    assert(errno == EINVAL);
    printf("PASSED\n");

    // Test 6: Successful mmap should return page-aligned address
    printf("Test 6: Successful mmap returns page-aligned address... ");
    result = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(result != MAP_FAILED);
    assert(((unsigned long)result % 4096) == 0);
    printf("PASSED\n");
    munmap(result, 4096);

    // =====================
    // SHARED MEMORY ERROR TESTS
    // =====================

    // Test 7: shmat with invalid shmid (-1)
    printf("Test 7: shmat with invalid shmid -1 (should fail)... ");
    errno = 0;
    result = shmat(-1, NULL, 0);
    assert(result == (void *)-1);
    printf("PASSED\n");

    // Test 8: shmat with non-existent shmid
    printf("Test 8: shmat with non-existent shmid (should fail)... ");
    errno = 0;
    result = shmat(999999, NULL, 0);
    assert(result == (void *)-1);
    printf("PASSED\n");

    return 0;
}
