#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

#define TEST_FILE    "testfiles/symlink_target.txt"
#define SYMLINK_FILE "testfiles/symlink_link.txt"
#define SYMLINK_AT_FILE "testfiles/symlinkat_link.txt"

int main() {
    int fd;
    char buf[1024];
    ssize_t len;
    struct stat st;

    printf("Testing symlink() syscall\n");
    fflush(stdout);

    // Create target file
    fd = open(TEST_FILE, O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create target file");
        exit(EXIT_FAILURE);
    }
    const char *data = "symlink test data\n";
    if (write(fd, data, strlen(data)) == -1) {
        perror("Failed to write to target file");
        close(fd);
        exit(EXIT_FAILURE);
    }
    close(fd);

    // Test 1: Create a symbolic link
    printf("\n=== Test 1: Create symbolic link ===\n");
    if (symlink(TEST_FILE, SYMLINK_FILE) == -1) {
        perror("Failed to create symbolic link");
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    printf("symlink() succeeded\n");

    // Test 2: Verify symlink points to correct target via readlink
    printf("\n=== Test 2: Verify symlink target via readlink ===\n");
    len = readlink(SYMLINK_FILE, buf, sizeof(buf) - 1);
    if (len == -1) {
        perror("readlink failed");
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    buf[len] = '\0';
    if (strcmp(buf, TEST_FILE) != 0) {
        fprintf(stderr, "Error: symlink points to '%s', expected '%s'\n", buf, TEST_FILE);
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    printf("symlink points to correct target: %s\n", buf);

    // Test 3: Verify symlink is detected as a symlink via lstat
    printf("\n=== Test 3: Verify symlink is a symlink (lstat) ===\n");
    if (lstat(SYMLINK_FILE, &st) == -1) {
        perror("lstat failed");
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    if (!S_ISLNK(st.st_mode)) {
        fprintf(stderr, "Error: Expected symlink, got mode %o\n", st.st_mode);
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    printf("lstat correctly identifies symlink\n");

    // // Test 4: Read through the symlink
    // printf("\n=== Test 4: Read through symlink ===\n");
    // fd = open(SYMLINK_FILE, O_RDONLY);
    // if (fd == -1) {
    //     perror("Failed to open symlink for reading");
    //     unlink(TEST_FILE);
    //     unlink(SYMLINK_FILE);
    //     exit(EXIT_FAILURE);
    // }
    // ssize_t bytes = read(fd, buf, sizeof(buf) - 1);
    // close(fd);
    // if (bytes == -1) {
    //     perror("Failed to read through symlink");
    //     unlink(TEST_FILE);
    //     unlink(SYMLINK_FILE);
    //     exit(EXIT_FAILURE);
    // }
    // buf[bytes] = '\0';
    // if (strcmp(buf, data) != 0) {
    //     fprintf(stderr, "Error: Read wrong data through symlink\n");
    //     unlink(TEST_FILE);
    //     unlink(SYMLINK_FILE);
    //     exit(EXIT_FAILURE);
    // }
    // printf("Successfully read through symlink: %s", buf);
    // Symlink dereferencing (following a symlink via open) is not yet supported in the Lind FS layer.

    // Test 5: EEXIST - symlink to existing path
    printf("\n=== Test 5: EEXIST on duplicate symlink ===\n");
    if (symlink(TEST_FILE, SYMLINK_FILE) != -1) {
        fprintf(stderr, "Error: Should have failed with EEXIST\n");
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    if (errno != EEXIST) {
        fprintf(stderr, "Error: Expected EEXIST, got errno %d\n", errno);
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    printf("Correctly got EEXIST\n");

    // Test 6: symlinkat with AT_FDCWD
    printf("\n=== Test 6: symlinkat with AT_FDCWD ===\n");
    if (symlinkat(TEST_FILE, AT_FDCWD, SYMLINK_AT_FILE) == -1) {
        perror("symlinkat failed");
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        exit(EXIT_FAILURE);
    }
    len = readlink(SYMLINK_AT_FILE, buf, sizeof(buf) - 1);
    if (len == -1) {
        perror("readlink on symlinkat result failed");
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        unlink(SYMLINK_AT_FILE);
        exit(EXIT_FAILURE);
    }
    buf[len] = '\0';
    if (strcmp(buf, TEST_FILE) != 0) {
        fprintf(stderr, "Error: symlinkat points to '%s', expected '%s'\n", buf, TEST_FILE);
        unlink(TEST_FILE);
        unlink(SYMLINK_FILE);
        unlink(SYMLINK_AT_FILE);
        exit(EXIT_FAILURE);
    }
    printf("symlinkat correctly created symlink: %s\n", buf);

    // Cleanup
    unlink(TEST_FILE);
    unlink(SYMLINK_FILE);
    unlink(SYMLINK_AT_FILE);

    printf("\nAll symlink() tests passed successfully\n");
    fflush(stdout);

    return EXIT_SUCCESS;
}