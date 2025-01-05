#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>

void test_readlink() {
    char buf[1024];
    ssize_t len;

    // Test Case 1: Valid symbolic link
    len = readlink("testfiles/readlinkfile", buf, sizeof(buf));
    if (len != -1) {
        buf[len] = '\0'; // Null-terminate the result to printout result
        printf("Test Case 1: Symbolic link points to: %s\n", buf);
    } else {
        perror("Test Case 1 failed");
    }

    // Test Case 2: Path is not a symbolic link
    len = readlink("testfiles/readlinkfile.txt", buf, sizeof(buf));
    if (len == -1) {
        printf("Test Case 2: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 2 failed: Unexpectedly succeeded\n");
    }

    // Test Case 3: Symbolic link with buffer too small
    len = readlink("testfiles/readlinkfile", buf, 5); // Intentionally small buffer
    if (len != -1) {
        printf("Test Case 3: Symbolic link truncated result: %.*s\n", (int)len, buf);
    } else {
        perror("Test Case 3 failed");
    }

    // Test Case 4: Non-existent path
    len = readlink("testfiles/readlink", buf, sizeof(buf));
    if (len == -1) {
        printf("Test Case 4: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 4 failed: Unexpectedly succeeded\n");
    }
}

int main() {
    test_readlink();
    return 0;
}
