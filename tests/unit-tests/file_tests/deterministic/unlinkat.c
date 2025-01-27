#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>
#include <sys/stat.h>

const char* TEST_DIR = "testfiles/";
const char* VALID_FILE = "testfiles/unlinkatfile.txt";
const char* NON_EXISTENT_FILE = "testfiles/nonexistent";
const char* VALID_SUBDIR = "testfiles/unlinkatsubdir";
const char* FILE_IN_SUBDIR = "testfiles/unlinkatsubdir/testfile.txt";

void create_test_environment() {
    // Create test directory and files
    mkdir(TEST_DIR, 0755);

    FILE* file = fopen(VALID_FILE, "w");
    if (file) {
        fputs("Test file content", file);
        fclose(file);
    } else {
        perror("Failed to create test file");
        exit(1);
    }

    mkdir(VALID_SUBDIR, 0755);

    file = fopen(FILE_IN_SUBDIR, "w");
    if (file) {
        fputs("Subdirectory test file content", file);
        fclose(file);
    } else {
        perror("Failed to create file in subdirectory");
        exit(1);
    }
}

void test_unlinkat() {
    int dirfd;
    int result;

    printf("\n=== Test Case 1: Remove valid file ===\n");
    result = unlinkat(AT_FDCWD, VALID_FILE, 0);
    if (result == 0) {
        printf("Test Case 1: Successfully removed %s\n", VALID_FILE);
    } else {
        perror("Test Case 1 failed");
    }

    printf("\n=== Test Case 2: Remove non-existent file ===\n");
    result = unlinkat(AT_FDCWD, NON_EXISTENT_FILE, 0);
    if (result == -1) {
        printf("Test Case 2: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 2 failed: Unexpectedly succeeded\n");
    }

    printf("\n=== Test Case 3: Remove file in a subdirectory ===\n");
    dirfd = open(VALID_SUBDIR, O_RDONLY);
    if (dirfd == -1) {
        perror("Failed to open subdirectory");
        return;
    }
    result = unlinkat(dirfd, "testfile.txt", 0);
    if (result == 0) {
        printf("Test Case 3: Successfully removed file in subdirectory\n");
    } else {
        perror("Test Case 3 failed");
    }
    close(dirfd);

    printf("\n=== Test Case 4: Remove a directory with AT_REMOVEDIR ===\n");
    result = unlinkat(AT_FDCWD, VALID_SUBDIR, AT_REMOVEDIR);
    if (result == 0) {
        printf("Test Case 4: Successfully removed directory %s\n", VALID_SUBDIR);
    } else {
        perror("Test Case 4 failed");
    }

    printf("\n=== Test Case 5: Remove a directory without AT_REMOVEDIR ===\n");
    mkdir(VALID_SUBDIR, 0755); // Recreate directory for testing
    result = unlinkat(AT_FDCWD, VALID_SUBDIR, 0);
    if (result == -1) {
        printf("Test Case 5: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 5 failed: Unexpectedly succeeded\n");
    }
}

int main() {
    create_test_environment();
    test_unlinkat();
    return 0;
}
