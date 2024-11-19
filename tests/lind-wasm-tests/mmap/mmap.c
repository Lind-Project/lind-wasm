#include <sys/mman.h> // import mmap()
#include <unistd.h>   // import NULL

int main() {
    const int pageSize = 65536; // 0x10000, according to wasi-libc
    const int numElements = pageSize / sizeof(int);

    // observe the linear memory length before mmap
    int linear_mem_end = __builtin_wasm_memory_size(0);

    // Allocate one page of memory
    int* addr = mmap(NULL, pageSize, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    
    if (addr == MAP_FAILED) {
        return 1; // 
    }

    // observe the current linear memory length
    linear_mem_end = __builtin_wasm_memory_size(0);

    // Write on the page
    for (int i = 0; i < numElements; i++) {
        addr[i] = i;
    }

    // Read to verify the writes are effective
    for (int i = 0; i < numElements; i++) {
        if (addr[i] != i) {
            munmap(addr, pageSize);
            return 1; // terminate early if read incorrect data
        }
    }

    munmap(addr, pageSize);
    return 0;
}
