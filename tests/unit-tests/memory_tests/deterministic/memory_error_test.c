#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/shm.h>
#include <errno.h>
#include <string.h>
#include <unistd.h>

int main() {
    int passed = 0;
    int failed = 0;
    void *result;
    int ret;

    printf("=== Memory Error Handling Test (mmap + shmat) ===\n\n");

    // Test 1: mmap with invalid file descriptor (not -1)
    // Note: This test checks if mmap properly rejects invalid file descriptors
    printf("Test 1: mmap with invalid file descriptor (should fail)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, 999, 0);
    if (result == MAP_FAILED) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected MAP_FAILED, got %p)\n", result);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    // Test 2: mmap with unaligned address
    printf("Test 2: mmap with unaligned address (should fail with EINVAL)... ");
    errno = 0;
    result = mmap((void *)0x1001, 4096, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (result == MAP_FAILED && errno == EINVAL) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected MAP_FAILED with EINVAL, got %p, errno=%d)\n", result, errno);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    // Test 3: mmap with unaligned offset
    printf("Test 3: mmap with unaligned offset (should fail with EINVAL)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS, -1, 123);
    if (result == MAP_FAILED && errno == EINVAL) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected MAP_FAILED with EINVAL, got %p, errno=%d)\n", result, errno);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    // Test 4: mmap with valid parameters (should succeed and return page-aligned address)
    printf("Test 4: mmap with valid parameters (should succeed)... ");
    result = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (result != MAP_FAILED && ((unsigned long)result % 4096) == 0) {
        printf("PASSED\n");
        passed++;
        munmap(result, 4096);
    } else {
        printf("FAILED (expected valid page-aligned address, got %p)\n", result);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    // Test 5: mmap with invalid flags combination
    printf("Test 5: mmap with both MAP_PRIVATE and MAP_SHARED (should fail with EINVAL)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE | MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    if (result == MAP_FAILED && errno == EINVAL) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected MAP_FAILED with EINVAL, got %p, errno=%d)\n", result, errno);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    // Test 6: mmap with neither MAP_PRIVATE nor MAP_SHARED
    printf("Test 6: mmap with neither MAP_PRIVATE nor MAP_SHARED (should fail with EINVAL)... ");
    errno = 0;
    result = mmap(NULL, 4096, PROT_READ, MAP_ANONYMOUS, -1, 0);
    if (result == MAP_FAILED && errno == EINVAL) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected MAP_FAILED with EINVAL, got %p, errno=%d)\n", result, errno);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    // Test 7: shmat with invalid shmid
    printf("Test 7: shmat with invalid shmid (should fail)... ");
    errno = 0;
    result = shmat(999999, NULL, 0);
    if (result == (void *)-1) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected -1, got %p, errno=%d)\n", result, errno);
        failed++;
        if (result != (void *)-1) {
            shmdt(result);
        }
    }

    // Test 8: shmget with invalid size (too large)
    printf("Test 8: shmget with size exceeding SHMMAX (should fail)... ");
    errno = 0;
    ret = shmget(IPC_PRIVATE, (size_t)-1, IPC_CREAT | 0666);
    if (ret == -1) {
        printf("PASSED\n");
        passed++;
    } else {
        printf("FAILED (expected -1, got %d, errno=%d)\n", ret, errno);
        failed++;
        if (ret >= 0) {
            shmctl(ret, IPC_RMID, NULL);
        }
    }

    // Test 9: Verify successful mmap returns page-aligned address
    printf("Test 9: Successful mmap returns page-aligned address... ");
    result = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (result != MAP_FAILED && ((unsigned long)result % 4096) == 0) {
        printf("PASSED\n");
        passed++;
        munmap(result, 4096);
    } else {
        printf("FAILED (expected page-aligned address, got %p)\n", result);
        failed++;
        if (result != MAP_FAILED) {
            munmap(result, 4096);
        }
    }

    printf("\n=== Test Summary ===\n");
    printf("Passed: %d\n", passed);
    printf("Failed: %d\n", failed);
    printf("Total:  %d\n", passed + failed);

    return (failed == 0) ? 0 : 1;
}

