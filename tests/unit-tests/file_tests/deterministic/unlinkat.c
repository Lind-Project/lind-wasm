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

// Set up the test environment by creating necessary directories and files
void create_test_environment() {
    // Create the test directory if it does not exist
    mkdir(TEST_DIR, 0755);

    // Create a test file with sample content
    FILE* file = fopen(VALID_FILE, "w");
    if (file) {
        fputs("Test file content", file);
        fclose(file);
    } else {
        perror("Failed to create test file");
        exit(1);
    }

    // Create a valid subdirectory for testing directory removal
    mkdir(VALID_SUBDIR, 0755);
}

// Test the unlinkat system call with different scenarios
void test_unlinkat() {
    int result;

    // Test Case 1: Remove a valid file
    printf("\n=== Test Case 1: Remove valid file ===\n");
    result = unlinkat(AT_FDCWD, VALID_FILE, 0);
    if (result == 0) {
        printf("Test Case 1: Successfully removed %s\n", VALID_FILE);
    } else {
        printf("Test Case 1 failed: %s\n", strerror(errno));
    }

    // Test Case 2: Attempt to remove a non-existent file
    printf("\n=== Test Case 2: Remove non-existent file ===\n");
    result = unlinkat(AT_FDCWD, NON_EXISTENT_FILE, 0);
    if (result == -1) {
        printf("Test Case 2: Expected failure: %d\n", result);
    } else {
        printf("Test Case 2 failed: Unexpectedly succeeded: %s\n", strerror(errno));
    }

    // Test Case 3: Remove a directory with the AT_REMOVEDIR flag
    printf("\n=== Test Case 3: Remove a directory with AT_REMOVEDIR ===\n");
    result = unlinkat(AT_FDCWD, VALID_SUBDIR, AT_REMOVEDIR);
    if (result == 0) {
        printf("Test Case 3: Successfully removed directory %s\n", VALID_SUBDIR);
    } else {
        printf("Test Case 3 failed: %s\n", strerror(errno));
    }

    // Test Case 4: Attempt to remove a directory without the AT_REMOVEDIR flag
    printf("\n=== Test Case 4: Remove a directory without AT_REMOVEDIR ===\n");
    mkdir(VALID_SUBDIR, 0755); // Recreate directory for testing
    result = unlinkat(AT_FDCWD, VALID_SUBDIR, 0);
    if (result == -1) {
        printf("Test Case 4: Expected failure: %s\n", strerror(errno));
    } else {
        printf("Test Case 4 failed: Unexpectedly succeeded\n");
    }
}

int main() {
    create_test_environment();  
    test_unlinkat();            
    return 0;
}
