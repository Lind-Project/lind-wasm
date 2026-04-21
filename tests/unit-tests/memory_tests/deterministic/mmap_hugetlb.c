// Test that MAP_HUGETLB flag is accepted by mmap (treated as hint, not error)
#include <sys/mman.h>
#include <stdio.h>

#define PAGESIZE 4096

#ifndef MAP_HUGETLB
#define MAP_HUGETLB 0x40000
#endif

int main(void) {
    size_t len = PAGESIZE;

    // MAP_HUGETLB is a performance hint - lind-wasm should accept it
    unsigned char *p = mmap(NULL, len, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS | MAP_HUGETLB, -1, 0);
    if (p == MAP_FAILED) {
        perror("mmap with MAP_HUGETLB");
        return 1;
    }

    // Verify memory is usable
    p[0] = 0xAB;
    p[len - 1] = 0xEF;

    if (p[0] != 0xAB || p[len - 1] != 0xEF) {
        fprintf(stderr, "memory check failed\n");
        munmap(p, len);
        return 2;
    }

    if (munmap(p, len) != 0) {
        perror("munmap");
        return 3;
    }

    printf("mmap MAP_HUGETLB test: PASS\n");
    return 0;
}
