/*
    Verify the page is invalid again after munmap
*/

#include <sys/mman.h> // For mmap()
#include <unistd.h>   // For close()

int main() {
    const int pageSize = 65536; // 64KB
    const int numElements = pageSize / sizeof(int);

    // Allocate one page of memory
    int* addr = mmap(NULL, pageSize, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    
    // Check if mmap failed
    if (addr == MAP_FAILED) {
        return 1; // Return with error code
    }

    // Write to the allocated memory
    for (int i = 0; i < numElements; i++) {
        addr[i] = i;
    }

    // Unmap the memory
    munmap(addr, pageSize);

    // Try to read from the unmapped memory
    volatile int test = addr[0]; // Potential segmentation fault or undefined behavior here

    return 0; // Return successfully (unlikely to reach this point without a fault)
}
