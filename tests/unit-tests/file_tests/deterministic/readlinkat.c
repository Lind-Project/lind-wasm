#include <unistd.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>

const char* VALID_SYMBOLIC_PATH = "testfiles/readlinkfile";
const char* NON_SYMBOLIC_PATH = "testfiles/fstatfile.txt";
const char* NON_EXISTENT_PATH = "testfiles/nonexistent";

void test_readlinkat() {
    char buf[1024];
    ssize_t len;

    // Test Case 1: Valid symbolic link with AT_FDCWD
    len = readlinkat(AT_FDCWD, VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    if (len != -1) {
        buf[len] = '\0'; // Null-terminate the result
        printf("Test Case 1: Symbolic link points to: %s\n", buf);
    } else {
        perror("Test Case 1 failed");
    }

    // Test Case 2: Valid symbolic link with a file descriptor
    int dirfd = open("testfiles/", O_RDONLY);
    if (dirfd == -1) {
        perror("Failed to open directory");
        return;
    }
    len = readlinkat(dirfd, VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    if (len != -1) {
        buf[len] = '\0'; // Null-terminate the result
        printf("Test Case 2: Symbolic link points to: %s\n", buf);
    } else {
        perror("Test Case 2 failed");
    }
    close(dirfd);

    // Test Case 3: Non-existent symbolic link
    len = readlinkat(AT_FDCWD, NON_EXISTENT_PATH, buf, sizeof(buf));
    if (len == -1) {
        printf("Test Case 3: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 3 failed: Unexpectedly succeeded\n");
    }

    // Test Case 4: Invalid file descriptor
    len = readlinkat(-1, VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    if (len == -1) {
        printf("Test Case 4: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 4 failed: Unexpectedly succeeded\n");
    }
}

int main() {
    test_readlinkat();
    return 0;
}
