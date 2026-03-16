// gcc -Wall -O2 mmap_test.c -o mmap_test
#include <sys/mman.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#define PAGESIZE 4096

int main(void) {
    size_t len = PAGESIZE;

    unsigned char *p = mmap(NULL, len, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        perror("mmap");
        assert(0);
    }

    p[0]       = 0xAB;
    p[len/2]   = 0xCD;
    p[len - 1] = 0xEF;

    if (p[0] != 0xAB || p[len/2] != 0xCD || p[len - 1] != 0xEF) {
        fprintf(stderr, "memory check failed\n");
        munmap(p, len);
        assert(0);
    }

    if (munmap(p, len) != 0) {
        perror("munmap");
        assert(0);
    }

    printf("mmap test: PASS\n");
    return 0;
}
