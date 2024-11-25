#include <unistd.h>
#include <stdlib.h> 
#include <string.h> 

int main() {
    const char* text = "Hello, world from Coulson's WASM malloc-hello!\n";

    int size = strlen(text) + 1;

    // Observe the ungrown memory
    int linear_mem_end = __builtin_wasm_memory_size(0);

    char* buffer = malloc(size); // the small size should incur sbrk path, not mmap path

    if (buffer == NULL) {
        return 1;
    }

    // if brk/sbrk called, this should gorwn a page
    linear_mem_end = __builtin_wasm_memory_size(0);

    strcpy(buffer, text);

    write(STDOUT_FILENO, buffer, size - 1);

    free(buffer);
    
    return 0;
}
