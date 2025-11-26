// Test: multiple successive mprotect calls
// Verifies correct state after overlapping protection changes
#include <sys/mman.h>
#include <stdio.h>
#include <assert.h>

#define PAGESIZE 4096
#define NUMPAGES 10

int main(void) {
    // Allocate 10 pages with READ|WRITE protection
    size_t len = PAGESIZE * NUMPAGES;
    unsigned char *p = mmap(NULL, len, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p != MAP_FAILED && "mmap failed");

    // Write initial data
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0x10 + i;
    }

    // First change: protect middle pages 3-6 to READ-only
    assert(mprotect(p + (3 * PAGESIZE), 4 * PAGESIZE, PROT_READ) == 0 && "first mprotect failed");

    // Second change: protect overlapping pages 5-7 to NONE
    assert(mprotect(p + (5 * PAGESIZE), 3 * PAGESIZE, PROT_NONE) == 0 && "second mprotect failed");

    // Verify we can still write to pages 0-2
    p[0] = 0x20;
    p[2 * PAGESIZE] = 0x22;

    // Verify we can read from pages 3-4 (READ-only from first mprotect)
    assert(p[3 * PAGESIZE] == 0x13 && "read from READ-only region failed");
    assert(p[4 * PAGESIZE] == 0x14 && "read from READ-only region failed");

    // Verify we can write to pages 8-9
    p[8 * PAGESIZE] = 0x28;
    p[9 * PAGESIZE] = 0x29;

    // Verify final state
    assert(p[0] == 0x20 && "final state verification failed");
    assert(p[2 * PAGESIZE] == 0x22 && "final state verification failed");
    assert(p[8 * PAGESIZE] == 0x28 && "final state verification failed");
    assert(p[9 * PAGESIZE] == 0x29 && "final state verification failed");

    assert(munmap(p, len) == 0 && "munmap failed");

    printf("mprotect_multiple_times test: PASS\n");
    return 0;
}

