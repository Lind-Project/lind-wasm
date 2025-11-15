// Test: multiple successive mprotect calls
// Verifies correct state after overlapping protection changes
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

    // Write initial data
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0x10 + i;
    }

    // First change: protect middle pages 3-6 to READ-only
    if (mprotect(p + (3 * PAGESIZE), 4 * PAGESIZE, PROT_READ) != 0) {
        perror("first mprotect failed");
        munmap(p, len);
        return 2;
    }

    // Second change: protect overlapping pages 5-7 to NONE
    if (mprotect(p + (5 * PAGESIZE), 3 * PAGESIZE, PROT_NONE) != 0) {
        perror("second mprotect failed");
        munmap(p, len);
        return 3;
    }

    // Verify we can still write to pages 0-2
    p[0] = 0x20;
    p[2 * PAGESIZE] = 0x22;

    // Verify we can read from pages 3-4 (READ-only from first mprotect)
    if (p[3 * PAGESIZE] != 0x13 || p[4 * PAGESIZE] != 0x14) {
        fprintf(stderr, "read from READ-only region failed\n");
        munmap(p, len);
        return 4;
    }

    // Verify we can write to pages 8-9
    p[8 * PAGESIZE] = 0x28;
    p[9 * PAGESIZE] = 0x29;

    // Verify final state
    if (p[0] != 0x20 || p[2 * PAGESIZE] != 0x22 ||
        p[8 * PAGESIZE] != 0x28 || p[9 * PAGESIZE] != 0x29) {
        fprintf(stderr, "final state verification failed\n");
        munmap(p, len);
        return 5;
    }

    if (munmap(p, len) != 0) {
        perror("munmap failed");
        return 6;
    }

    printf("mprotect_multiple_times test: PASS\n");
    return 0;
}

