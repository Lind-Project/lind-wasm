// Test: mmap with alignment requirements
// Verifies proper alignment when using MAP_FIXED and specific addresses
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdint.h>

#define PAGESIZE 4096
#define ALIGNMENT (8 * PAGESIZE)  // 8-page alignment

int main(void) {
    // First, allocate a large region to find suitable aligned address
    size_t large_size = 100 * PAGESIZE;
    unsigned char *large = mmap(NULL, large_size, PROT_READ | PROT_WRITE,
                                MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (large == MAP_FAILED) {
        perror("large mmap failed");
        return 1;
    }

    // Find an 8-page aligned address within the large region
    uintptr_t addr = (uintptr_t)large;
    uintptr_t aligned_addr = (addr + ALIGNMENT - 1) & ~(ALIGNMENT - 1);
    
    // Ensure we have space
    if (aligned_addr + (10 * PAGESIZE) > addr + large_size) {
        fprintf(stderr, "not enough space for aligned allocation\n");
        munmap(large, large_size);
        return 2;
    }

    // Unmap the large region
    munmap(large, large_size);

    // Now allocate at the aligned address with MAP_FIXED
    void *p = mmap((void *)aligned_addr, 10 * PAGESIZE, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (p == MAP_FAILED) {
        perror("aligned mmap with MAP_FIXED failed");
        return 3;
    }

    // Verify the address is aligned
    if ((uintptr_t)p % ALIGNMENT != 0) {
        fprintf(stderr, "allocated address is not properly aligned\n");
        fprintf(stderr, "address: %p, alignment: %d, modulo: %lu\n",
                p, ALIGNMENT, (uintptr_t)p % ALIGNMENT);
        munmap(p, 10 * PAGESIZE);
        return 4;
    }

    // Verify we got the address we requested
    if (p != (void *)aligned_addr) {
        fprintf(stderr, "MAP_FIXED didn't honor requested address\n");
        fprintf(stderr, "requested: %p, got: %p\n", (void *)aligned_addr, p);
        munmap(p, 10 * PAGESIZE);
        return 5;
    }

    // Write and verify data
    unsigned char *bytes = (unsigned char *)p;
    for (int i = 0; i < 10; i++) {
        bytes[i * PAGESIZE] = 0xA0 + i;
    }

    for (int i = 0; i < 10; i++) {
        if (bytes[i * PAGESIZE] != 0xA0 + i) {
            fprintf(stderr, "data verification failed at page %d\n", i);
            munmap(p, 10 * PAGESIZE);
            return 6;
        }
    }

    if (munmap(p, 10 * PAGESIZE) != 0) {
        perror("munmap failed");
        return 7;
    }

    printf("mmap_aligned test: PASS\n");
    return 0;
}

