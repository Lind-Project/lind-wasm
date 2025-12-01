/*
    Verfiy if accessing unallocated memory cause a problem
*/

#include <sys/mman.h> // import mmap()
#include <unistd.h>   // import NULL

int main() {
    const int pageSize = 65536; // 0x10000, according to wasi-libc
    const int numElements = pageSize / sizeof(int);

    // Allocate one page of memory
    int* addr = mmap(NULL, pageSize, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    
    if (addr == MAP_FAILED) {
        return 1;
    }

    // Attempt to write beyond the allocated region,
    // this should cause an error since we are accessing unallocated memory
    for (int i = 0; i < numElements * 3; i++) {
        addr[i] = i;
    }

    munmap(addr, pageSize);
    return 0;
}
