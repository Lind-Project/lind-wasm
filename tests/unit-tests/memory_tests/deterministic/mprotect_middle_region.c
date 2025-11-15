// Test: mprotect on middle of memory region
// Verifies correct three-way splitting when changing protection in middle
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

    // Write to all pages
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xCC + i;
    }

    // Change protection on middle 4 pages (pages 3-6) to READ-only
    if (mprotect(p + (3 * PAGESIZE), 4 * PAGESIZE, PROT_READ) != 0) {
        perror("mprotect failed");
        munmap(p, len);
        return 2;
    }

    // Verify we can read from middle protected region
    if (p[3 * PAGESIZE] != 0xCC + 3 || p[6 * PAGESIZE] != 0xCC + 6) {
        fprintf(stderr, "read from protected middle region failed\n");
        munmap(p, len);
        return 3;
    }

    // Verify we can write to pages before protected region (0-2)
    for (int i = 0; i < 3; i++) {
        p[i * PAGESIZE] = 0xDD + i;
    }

    // Verify we can write to pages after protected region (7-9)
    for (int i = 7; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xDD + i;
    }

    // Verify all writes succeeded
    if (p[0] != 0xDD || p[2 * PAGESIZE] != 0xDD + 2 ||
        p[7 * PAGESIZE] != 0xDD + 7 || p[9 * PAGESIZE] != 0xDD + 9) {
        fprintf(stderr, "write to unprotected regions failed\n");
        munmap(p, len);
        return 4;
    }

    if (munmap(p, len) != 0) {
        perror("munmap failed");
        return 5;
    }

    printf("mprotect_middle_region test: PASS\n");
    return 0;
}

