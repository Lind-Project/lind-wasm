#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>

int main() {
    // Define the size of the shared memory
    size_t mem_size = 1024;

    // Create shared memory region using mmap
    char *shared_mem = (char*)mmap(NULL, mem_size, PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    if (shared_mem == MAP_FAILED) {
        perror("mmap");
        exit(EXIT_FAILURE);
    }

    // Fork a child process
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        munmap(shared_mem, mem_size);
        exit(EXIT_FAILURE);
    }

    if (pid == 0) {
        // Child process
        printf("Child: Writing to shared memory.\n");
        const char *child_message = "Hello from the child process!";
        strncpy(shared_mem, child_message, mem_size);

        // Sleep to simulate some work
        sleep(2);

        printf("Child: Reading from shared memory: '%s'\n", shared_mem);

        // Unmap shared memory in the child
        if (munmap(shared_mem, mem_size) != 0) {
            perror("munmap in child");
            exit(EXIT_FAILURE);
        }

        printf("Child: Exiting.\n");
    } else {
        // Parent process
        printf("Parent: Waiting for child to write.\n");

        // Sleep to simulate waiting for the child
        sleep(1);

        printf("Parent: Reading from shared memory: '%s'\n", shared_mem);

        const char *parent_message = "Hello from the parent process!";
        strncpy(shared_mem, parent_message, mem_size);

        // Wait for the child to finish
        wait(NULL);

        printf("Parent: Reading modified shared memory: '%s'\n", shared_mem);

        // Unmap shared memory in the parent
        if (munmap(shared_mem, mem_size) != 0) {
            perror("munmap in parent");
            exit(EXIT_FAILURE);
        }

        printf("Parent: Exiting.\n");
    }

    return 0;
}
