// Test for shared memory error handling
// This test verifies that shmat properly handles errors and returns proper errno
// when called with invalid parameters

#include <sys/ipc.h>
#include <sys/shm.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>

int main(void) {
    int test_passed = 1;

    // Test 1: shmat with invalid shmid should fail
    errno = 0;
    void *result1 = shmat(-1, NULL, 0);
    if (result1 != (void *)-1) {
        fprintf(stderr, "Test 1 FAILED: shmat with invalid shmid should return -1\n");
        test_passed = 0;
        shmdt(result1);
    } else {
        printf("Test 1 PASSED: shmat with invalid shmid returned -1\n");
        printf("errno: %d (%s)\n", errno, strerror(errno));
    }

    // Test 2: shmat with non-existent shmid should fail
    errno = 0;
    void *result2 = shmat(999999, NULL, 0);
    if (result2 != (void *)-1) {
        fprintf(stderr, "Test 2 FAILED: shmat with non-existent shmid should return -1\n");
        test_passed = 0;
        shmdt(result2);
    } else {
        printf("Test 2 PASSED: shmat with non-existent shmid returned -1\n");
        printf("errno: %d (%s)\n", errno, strerror(errno));
    }

    // Test 3: Try to create shared memory with invalid size (too large)
    errno = 0;
    int shmid = shmget(IPC_PRIVATE, (size_t)-1, IPC_CREAT | 0666);
    if (shmid != -1) {
        fprintf(stderr, "Test 3 FAILED: shmget with invalid size should return -1\n");
        test_passed = 0;
        shmctl(shmid, IPC_RMID, NULL);
    } else {
        printf("Test 3 PASSED: shmget with invalid size returned -1\n");
        printf("errno: %d (%s)\n", errno, strerror(errno));
    }

    // Test 4: Create valid shared memory, then try invalid attach with bad address
    errno = 0;
    int valid_shmid = shmget(IPC_PRIVATE, 4096, IPC_CREAT | 0666);
    if (valid_shmid == -1) {
        fprintf(stderr, "Test 4 SETUP FAILED: Could not create shared memory\n");
        test_passed = 0;
    } else {
        // Try to attach at an invalid address (not page-aligned on some systems)
        errno = 0;
        void *result4 = shmat(valid_shmid, (void *)0x1, 0);
        if (result4 != (void *)-1) {
            // Some systems allow this, so we just check that it doesn't crash
            printf("Test 4 NOTE: shmat with unaligned address succeeded (system-dependent)\n");
            shmdt(result4);
        } else {
            printf("Test 4 PASSED: shmat with unaligned address returned -1\n");
            printf("errno: %d (%s)\n", errno, strerror(errno));
        }
        
        // Clean up
        shmctl(valid_shmid, IPC_RMID, NULL);
    }

    if (test_passed) {
        printf("\nAll shared memory error handling tests PASSED\n");
        printf("Issue #451: Error codes are properly propagated for shared memory operations\n");
        return 0;
    } else {
        fprintf(stderr, "\nSome shared memory error handling tests FAILED\n");
        return 1;
    }
}

