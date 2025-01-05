#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>

const char* VALID_SYMBOLIC_PATH = "testfiles/readlinkfile";
const char* NON_SYMBOLIC_PATH = "testfiles/fstatfile.txt";
const char* NON_EXISTENT_PATH = "testfiles/nonexistent";

void test_readlink() {
    char buf[1024];
    ssize_t len;

    // Test Case 1: Valid symbolic link
    printf("\n=== Test Case 1: Valid symbolic link ===\n");
    len = readlink(VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    if (len != -1) {
        buf[len] = '\0'; // Null-terminate the result to print the result
        printf("Symbolic link points to: %s\n", buf);
    } else {
        perror("Test Case 1 failed");
    }

    // Test Case 2: Path is not a symbolic link
    printf("\n=== Test Case 2: Path is not a symbolic link ===\n");
    len = readlink(NON_SYMBOLIC_PATH, buf, sizeof(buf));
    if (len == -1) {
        printf("Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 2 failed: Unexpectedly succeeded\n");
    }

    // Test Case 3: Symbolic link with buffer too small
    printf("\n=== Test Case 3: Symbolic link with buffer too small ===\n");
    len = readlink(VALID_SYMBOLIC_PATH, buf, 5); // Intentionally small buffer
    if (len != -1) {
        printf("Symbolic link truncated result: %.*s\n", (int)len, buf);
    } else {
        perror("Test Case 3 failed");
    }

    // Test Case 4: Non-existent path
    printf("\n=== Test Case 4: Non-existent path ===\n");
    len = readlink(NON_EXISTENT_PATH, buf, sizeof(buf));
    if (len == -1) {
        printf("Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 4 failed: Unexpectedly succeeded\n");
    }
}

int main() {
    test_readlink();
    return 0;
}
