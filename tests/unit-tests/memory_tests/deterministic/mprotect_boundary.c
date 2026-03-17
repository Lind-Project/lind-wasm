// Test: mprotect on exact page boundaries
// Verifies correct handling of single-page and precise boundary modifications
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

    // Write test data
    for (int i = 0; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0x80 + i;
    }

    // Change protection on a single page in the middle (page 5)
    assert(mprotect(p + (5 * PAGESIZE), PAGESIZE, PROT_READ) == 0 && "mprotect single page failed");

    // Verify we can read from the protected page
    assert(p[5 * PAGESIZE] == 0x85 && "read from single protected page failed");

    // Verify we can write to pages before (0-4)
    for (int i = 0; i < 5; i++) {
        p[i * PAGESIZE] = 0x90 + i;
    }

    // Verify we can write to pages after (6-9)
    for (int i = 6; i < NUMPAGES; i++) {
        p[i * PAGESIZE] = 0x90 + i;
    }

    // Verify the writes
    assert(p[0] == 0x90 && "boundary write verification failed");
    assert(p[4 * PAGESIZE] == 0x94 && "boundary write verification failed");
    assert(p[6 * PAGESIZE] == 0x96 && "boundary write verification failed");
    assert(p[9 * PAGESIZE] == 0x99 && "boundary write verification failed");

    // Change protection on first page only
    assert(mprotect(p, PAGESIZE, PROT_READ) == 0 && "mprotect first page failed");

    // Verify we can read from first page
    assert(p[0] == 0x90 && "read from first protected page failed");

    // Change protection on last page only
    assert(mprotect(p + (9 * PAGESIZE), PAGESIZE, PROT_READ) == 0 && "mprotect last page failed");

    // Verify we can read from last page
    assert(p[9 * PAGESIZE] == 0x99 && "read from last protected page failed");

    assert(munmap(p, len) == 0 && "munmap failed");

    printf("mprotect_boundary test: PASS\n");
    return 0;
}

