#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>
#include <stdlib.h>
#include <sys/wait.h> 

int main() {
    // Open a file with read/write permissions, create if it doesn’t exist, and truncate if it does.
    int fd = open("testfile.txt", O_RDWR | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        perror("open");
        return 1;
    }

    // Use `dup3` to duplicate the file descriptor `fd` to `fd + 1` and set the O_CLOEXEC flag.
    // O_CLOEXEC ensures that the duplicated file descriptor is automatically closed on exec().
    int newfd = dup3(fd, fd + 1, O_CLOEXEC);
    if (newfd < 0) {
        perror("dup3");
        return 1;
    }

    printf("Before exec: oldfd=%d, newfd=%d (should be open)\n", fd, newfd);

    // Fork a new process
    pid_t pid = fork();
    if (pid == 0) { 
        // **Child Process: Execute a new program**
        printf("[CHILD] Running exec...\n");

        // `exec()` replaces the current process with `/bin/cat /dev/null`, keeping the process running.
        char *args[] = {"/bin/cat", "/dev/null", NULL};
        execv(args[0], args);

        // If exec fails, this code executes (which should not happen if exec is successful).
        perror("[CHILD] execv failed");
        exit(1);
    }

    // Parent Process: Wait for the child to finish
    wait(NULL);

    //Test if O_CLOEXEC works
    printf("[PARENT] Testing write after exec...\n");
    if (write(newfd, "Test O_CLOEXEC\n", 15) < 0) {
        perror("[PARENT] write to newfd failed (expected if O_CLOEXEC works)");
    } else {
        printf("[PARENT] write to newfd succeeded (O_CLOEXEC NOT working!)\n");
    }

    printf("[PARENT] Test completed.\n");

    // Close the original and duplicated file descriptors
    close(fd);
    close(newfd);
    return 0;
}



