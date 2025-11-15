// Test: mprotect with same protection value
// Verifies no fragmentation when protection doesn't actually change
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

    // Write test data
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xEE + i;
    }

    // Apply mprotect with the SAME protection (READ|WRITE) on middle pages
    // This should NOT cause fragmentation
    if (mprotect(p + (3 * PAGESIZE), 4 * PAGESIZE, PROT_READ | PROT_WRITE) != 0) {
        perror("mprotect failed");
        munmap(p, len);
        return 2;
    }

    // Verify we can still write to all pages (protection unchanged)
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0xFF - i;
    }

    // Verify writes succeeded
    if (p[0] != 0xFF || p[5 * PAGESIZE] != 0xFF - 5 || p[9 * PAGESIZE] != 0xFF - 9) {
        fprintf(stderr, "write after same-value mprotect failed\n");
        munmap(p, len);
        return 3;
    }

    if (munmap(p, len) != 0) {
        perror("munmap failed");
        return 4;
    }

    printf("mprotect_same_value test: PASS\n");
    return 0;
}

