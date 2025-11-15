// Test: mprotect on exact page boundaries
// Verifies correct handling of single-page and precise boundary modifications
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
        p[i * PAGESIZE] = 0x80 + i;
    }

    // Change protection on a single page in the middle (page 5)
    if (mprotect(p + (5 * PAGESIZE), PAGESIZE, PROT_READ) != 0) {
        perror("mprotect single page failed");
        munmap(p, len);
        return 2;
    }

    // Verify we can read from the protected page
    if (p[5 * PAGESIZE] != 0x85) {
        fprintf(stderr, "read from single protected page failed\n");
        munmap(p, len);
        return 3;
    }

    // Verify we can write to pages before (0-4)
    for (int i = 0; i < 5; i++) {
        p[i * PAGESIZE] = 0x90 + i;
    }

    // Verify we can write to pages after (6-9)
    for (int i = 6; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0x90 + i;
    }

    // Verify the writes
    if (p[0] != 0x90 || p[4 * PAGESIZE] != 0x94 ||
        p[6 * PAGESIZE] != 0x96 || p[9 * PAGESIZE] != 0x99) {
        fprintf(stderr, "boundary write verification failed\n");
        munmap(p, len);
        return 4;
    }

    // Change protection on first page only
    if (mprotect(p, PAGESIZE, PROT_READ) != 0) {
        perror("mprotect first page failed");
        munmap(p, len);
        return 5;
    }

    // Verify we can read from first page
    if (p[0] != 0x90) {
        fprintf(stderr, "read from first protected page failed\n");
        munmap(p, len);
        return 6;
    }

    // Change protection on last page only
    if (mprotect(p + (9 * PAGESIZE), PAGESIZE, PROT_READ) != 0) {
        perror("mprotect last page failed");
        munmap(p, len);
        return 7;
    }

    // Verify we can read from last page
    if (p[9 * PAGESIZE] != 0x99) {
        fprintf(stderr, "read from last protected page failed\n");
        munmap(p, len);
        return 8;
    }

    if (munmap(p, len) != 0) {
        perror("munmap failed");
        return 9;
    }

    printf("mprotect_boundary test: PASS\n");
    return 0;
}

