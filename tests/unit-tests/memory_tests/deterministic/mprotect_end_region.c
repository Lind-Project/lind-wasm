// Test: mprotect on end of memory region
// Verifies correct splitting when changing protection at end of mapped region
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

    // Write to all pages to ensure they work
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xAA + i;
    }

    // Change protection on last 3 pages to READ-only
    assert(mprotect(p + (7 * PAGESIZE), 3 * PAGESIZE, PROT_READ) == 0 && "mprotect failed");

    // Verify we can still read from last 3 pages
    assert(p[7 * PAGESIZE] == 0xAA + 7 && "read from protected region failed");
    assert(p[9 * PAGESIZE] == 0xAA + 9 && "read from protected region failed");

    // Verify we can still write to first 7 pages
    for (int i = 0; i < 7; i++) {
        p[i * PAGESIZE] = 0xBB + i;
    }

    // Verify writes succeeded
    assert(p[0] == 0xBB && "write to unprotected region failed");
    assert(p[6 * PAGESIZE] == 0xBB + 6 && "write to unprotected region failed");

    assert(munmap(p, len) == 0 && "munmap failed");

    printf("mprotect_end_region test: PASS\n");
    return 0;
}

