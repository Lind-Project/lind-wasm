// Test: mmap with alignment requirements
// Verifies proper page alignment for memory allocations
#include <sys/mman.h>
#include <stdio.h>
#include <assert.h>
#include <stdint.h>

#define PAGESIZE 4096
#define TEST_PAGES 10

int main(void) {
    size_t alloc_size = TEST_PAGES * PAGESIZE;
    
    // Test 1: Basic allocation with page alignment verification
    unsigned char *p1 = mmap(NULL, alloc_size, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p1 != MAP_FAILED && "first mmap failed");
    
    // All mmap allocations should be page-aligned
    assert((uintptr_t)p1 % PAGESIZE == 0 && "first allocation not page-aligned");
    
    // Write and verify data
    for (int i = 0; i < TEST_PAGES; i++) {
        p1[i * PAGESIZE] = 0xA0 + i;
    }
    for (int i = 0; i < TEST_PAGES; i++) {
        assert(p1[i * PAGESIZE] == 0xA0 + i && "first allocation data verification failed");
    }
    
    // Test 2: Second allocation should also be page-aligned
    unsigned char *p2 = mmap(NULL, alloc_size, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p2 != MAP_FAILED && "second mmap failed");
    assert((uintptr_t)p2 % PAGESIZE == 0 && "second allocation not page-aligned");
    
    // Write and verify data for second allocation
    for (int i = 0; i < TEST_PAGES; i++) {
        p2[i * PAGESIZE] = 0xB0 + i;
    }
    for (int i = 0; i < TEST_PAGES; i++) {
        assert(p2[i * PAGESIZE] == 0xB0 + i && "second allocation data verification failed");
    }
    
    // Test 3: Third allocation to verify consistency
    unsigned char *p3 = mmap(NULL, alloc_size, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(p3 != MAP_FAILED && "third mmap failed");
    assert((uintptr_t)p3 % PAGESIZE == 0 && "third allocation not page-aligned");
    
    // Cleanup
    assert(munmap(p3, alloc_size) == 0 && "munmap p3 failed");
    assert(munmap(p2, alloc_size) == 0 && "munmap p2 failed");
    assert(munmap(p1, alloc_size) == 0 && "munmap p1 failed");
    
    printf("mmap_aligned test: PASS\n");
    return 0;
}

