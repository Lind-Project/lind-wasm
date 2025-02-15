#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>
#include <string.h>

#define FILE_PATH "example.txt"
#define FILE_SIZE 4096

int main() {
    // Create or open a file
    int fd = open(FILE_PATH, O_RDWR | O_CREAT, 0666);
    if (fd == -1) {
        perror("open");
        exit(EXIT_FAILURE);
    }

    // Ensure the file has the desired size
    if (ftruncate(fd, FILE_SIZE) == -1) {
        perror("ftruncate");
        close(fd);
        exit(EXIT_FAILURE);
    }

    // Map the file into memory
    void* addr = mmap(NULL, FILE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (addr == MAP_FAILED) {
        perror("mmap");
        close(fd);
        exit(EXIT_FAILURE);
    }

    // Close the file descriptor as it's no longer needed
    close(fd);

    // Write data to the mapped memory
    const char* message = "Hello, mmap!\0";
    memcpy(addr, message, strlen(message));

    printf("Data written to memory-mapped file: %s\n", (char*)addr);

    // Read back the data from the mapped memory
    printf("Data read back from memory-mapped file: %s\n", (char*)addr);

    // Unmap the memory
    if (munmap(addr, FILE_SIZE) == -1) {
        perror("munmap");
        exit(EXIT_FAILURE);
    }

    return 0;
}
