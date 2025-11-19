// Test: mprotect on middle of memory region
// Verifies correct three-way splitting when changing protection in middle
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

    // Write to all pages
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xCC + i;
    }

    // Change protection on middle 4 pages (pages 3-6) to READ-only
    assert(mprotect(p + (3 * PAGESIZE), 4 * PAGESIZE, PROT_READ) == 0 && "mprotect failed");

    // Verify we can read from middle protected region
    assert(p[3 * PAGESIZE] == 0xCC + 3 && "read from protected middle region failed");
    assert(p[6 * PAGESIZE] == 0xCC + 6 && "read from protected middle region failed");

    // Verify we can write to pages before protected region (0-2)
    for (int i = 0; i < 3; i++) {
        p[i * PAGESIZE] = 0xDD + i;
    }

    // Verify we can write to pages after protected region (7-9)
    for (int i = 7; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xDD + i;
    }

    // Verify all writes succeeded
    assert(p[0] == 0xDD && "write to unprotected regions failed");
    assert(p[2 * PAGESIZE] == 0xDD + 2 && "write to unprotected regions failed");
    assert(p[7 * PAGESIZE] == 0xDD + 7 && "write to unprotected regions failed");
    assert(p[9 * PAGESIZE] == 0xDD + 9 && "write to unprotected regions failed");

    assert(munmap(p, len) == 0 && "munmap failed");

    printf("mprotect_middle_region test: PASS\n");
    return 0;
}

