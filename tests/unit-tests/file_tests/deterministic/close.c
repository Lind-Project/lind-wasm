#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>

#define FILE_PATH "testfiles/close.txt"
#define ITERATIONS 2000

int main(void) {
    int fd;

    // Ensure the test file exists
    fd = open(FILE_PATH, O_CREAT | O_WRONLY, 0777);
    if (fd == -1) {
        // Keep stderr noise minimal for harness; nonzero exit signals failure
        return 2;
    }
    close(fd);

    for (int i = 0; i < ITERATIONS; i++) {
        fd = open(FILE_PATH, O_RDONLY);
        if (fd == -1) {
            return 3;
        }

        if (close(fd) == -1) {
            return 4;
        }

        // After closing, the fd must be invalid: fcntl should fail with EBADF
        errno = 0;
        if (fcntl(fd, F_GETFD) != -1 || errno != EBADF) {
            // If a runtime reuses fds aggressively, this check still happens
            // before the next open, so fd must be invalid here.
            return 5;
        }
    }

    // Success output (must match expected file exactly)
    printf("File opened and closed %d times successfully.\n", ITERATIONS);
    fflush(stdout);

    // Cleanup (ignore errors to keep output deterministic)
    unlink(FILE_PATH);
    return 0;
}

