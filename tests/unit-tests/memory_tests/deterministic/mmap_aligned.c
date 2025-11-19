// Test: mmap with alignment requirements
// Verifies proper alignment when using MAP_FIXED and specific addresses
#include <sys/mman.h>
#include <stdio.h>
#include <assert.h>
#include <stdint.h>

#define PAGESIZE 4096
#define ALIGNMENT (8 * PAGESIZE)  // 8-page alignment

int main(void) {
    // First, allocate a large region to find suitable aligned address
    size_t large_size = 100 * PAGESIZE;
    unsigned char *large = mmap(NULL, large_size, PROT_READ | PROT_WRITE,
                                MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(large != MAP_FAILED && "large mmap failed");

    // Find an 8-page aligned address within the large region
    uintptr_t addr = (uintptr_t)large;
    uintptr_t aligned_addr = (addr + ALIGNMENT - 1) & ~(ALIGNMENT - 1);
    
    // Ensure we have space
    assert(aligned_addr + (10 * PAGESIZE) <= addr + large_size && "not enough space for aligned allocation");

    // Unmap the large region
    assert(munmap(large, large_size) == 0 && "munmap large failed");

    // Now allocate at the aligned address with MAP_FIXED
    void *p = mmap((void *)aligned_addr, 10 * PAGESIZE, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    assert(p != MAP_FAILED && "aligned mmap with MAP_FIXED failed");

    // Verify the address is aligned
    assert((uintptr_t)p % ALIGNMENT == 0 && "allocated address is not properly aligned");

    // Verify we got the address we requested
    assert(p == (void *)aligned_addr && "MAP_FIXED didn't honor requested address");

    // Write and verify data
    unsigned char *bytes = (unsigned char *)p;
    for (int i = 0; i < 10; i++) {
        bytes[i * PAGESIZE] = 0xA0 + i;
    }

    for (int i = 0; i < 10; i++) {
        assert(bytes[i * PAGESIZE] == 0xA0 + i && "data verification failed");
    }

    assert(munmap(p, 10 * PAGESIZE) == 0 && "munmap failed");

    printf("mmap_aligned test: PASS\n");
    return 0;
}

