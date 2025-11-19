// Test: mprotect spanning multiple separate memory regions
// Verifies correct handling when protection change spans multiple mappings
#include <sys/mman.h>
#include <stdio.h>
#include <assert.h>

#define PAGESIZE 4096
#define REGION_SIZE (10 * PAGESIZE)

int main(void) {
    // Allocate three separate memory regions
    unsigned char *p1 = mmap(NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p1 != MAP_FAILED && "first mmap failed");

    unsigned char *p2 = mmap(NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p2 != MAP_FAILED && "second mmap failed");

    unsigned char *p3 = mmap(NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p3 != MAP_FAILED && "third mmap failed");

    // Write test data to all three regions
    for (int i = 0; i < 10; i++) {
        p1[i * PAGESIZE] = 0x31 + i;
        p2[i * PAGESIZE] = 0x41 + i;
        p3[i * PAGESIZE] = 0x51 + i;
    }

    // Change protection on parts of each region
    // Last 5 pages of p1 to READ-only
    assert(mprotect(p1 + (5 * PAGESIZE), 5 * PAGESIZE, PROT_READ) == 0 && "mprotect p1 failed");

    // All of p2 to READ-only
    assert(mprotect(p2, REGION_SIZE, PROT_READ) == 0 && "mprotect p2 failed");

    // First 5 pages of p3 to READ-only
    assert(mprotect(p3, 5 * PAGESIZE, PROT_READ) == 0 && "mprotect p3 failed");

    // Verify we can write to unprotected parts of p1 (first 5 pages)
    for (int i = 0; i < 5; i++) {
        p1[i * PAGESIZE] = 0x61 + i;
    }

    // Verify we can read from protected parts of p1 (last 5 pages)
    assert(p1[5 * PAGESIZE] == 0x36 && "read from p1 protected region failed");
    assert(p1[9 * PAGESIZE] == 0x3A && "read from p1 protected region failed");

    // Verify we can read from p2 (all READ-only)
    assert(p2[0] == 0x41 && "read from p2 failed");
    assert(p2[9 * PAGESIZE] == 0x4A && "read from p2 failed");

    // Verify we can write to unprotected parts of p3 (last 5 pages)
    for (int i = 5; i < 10; i++) {
        p3[i * PAGESIZE] = 0x71 + i;
    }

    // Verify we can read from protected parts of p3 (first 5 pages)
    assert(p3[0] == 0x51 && "read from p3 protected region failed");
    assert(p3[4 * PAGESIZE] == 0x55 && "read from p3 protected region failed");

    // Cleanup
    assert(munmap(p1, REGION_SIZE) == 0 && "munmap p1 failed");
    assert(munmap(p2, REGION_SIZE) == 0 && "munmap p2 failed");
    assert(munmap(p3, REGION_SIZE) == 0 && "munmap p3 failed");

    printf("mprotect_spanning_regions test: PASS\n");
    return 0;
}

