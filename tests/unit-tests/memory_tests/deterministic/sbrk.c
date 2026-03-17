#include <stdio.h>
#include <unistd.h>
#include <string.h>

int main() {
    // Allocate memory using sbrk
    size_t size = 1024; // Allocate 1024 bytes
    void *initial_brk = sbrk(0); // Get the current program break
    void *new_brk = sbrk(size);  // Increment the program break

    if (new_brk == (void *)-1) {
        perror("sbrk failed");
        return 1;
    }

    // Use the allocated memory
    char *buffer = (char *)new_brk;
    strcpy(buffer, "Hello, sbrk memory!");
    printf("Content in allocated memory: %s\n", buffer);

    // Deallocate memory by moving the program break back
    if (sbrk(-size) == (void *)-1) {
        perror("sbrk failed to deallocate");
        return 1;
    }

    return 0;
}
