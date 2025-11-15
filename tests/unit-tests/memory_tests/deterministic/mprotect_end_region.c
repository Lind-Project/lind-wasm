// Test: mprotect on end of memory region
// Verifies correct splitting when changing protection at end of mapped region
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

#define PAGESIZE 4096
#define NUMPAGES 10

int main(void) {
    // Allocate 10 pages with READ|WRITE protection
    size_t len = PAGESIZE * NUMPAGES;
    unsigned char *p = mmap(NULL, len, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        perror("mmap failed");
        return 1;
    }

    // Write to all pages to ensure they work
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xAA + i;
    }

    // Change protection on last 3 pages to READ-only
    if (mprotect(p + (7 * PAGESIZE), 3 * PAGESIZE, PROT_READ) != 0) {
        perror("mprotect failed");
        munmap(p, len);
        return 2;
    }

    // Verify we can still read from last 3 pages
    if (p[7 * PAGESIZE] != 0xAA + 7 || p[9 * PAGESIZE] != 0xAA + 9) {
        fprintf(stderr, "read from protected region failed\n");
        munmap(p, len);
        return 3;
    }

    // Verify we can still write to first 7 pages
    for (int i = 0; i < 7; i++) {
        p[i * PAGESIZE] = 0xBB + i;
    }

    // Verify writes succeeded
    if (p[0] != 0xBB || p[6 * PAGESIZE] != 0xBB + 6) {
        fprintf(stderr, "write to unprotected region failed\n");
        munmap(p, len);
        return 4;
    }

    if (munmap(p, len) != 0) {
        perror("munmap failed");
        return 5;
    }

    printf("mprotect_end_region test: PASS\n");
    return 0;
}

