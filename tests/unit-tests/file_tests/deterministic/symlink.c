#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

#define TEST_FILE       "/testfiles/symlink_target.txt"
#define SYMLINK_FILE    "/testfiles/symlink_link.txt"
#define SYMLINK_AT_FILE "/testfiles/symlinkat_link.txt"

static void cleanup() {
    unlink(TEST_FILE);
    unlink(SYMLINK_FILE);
    unlink(SYMLINK_AT_FILE);
}

int main() {
    int fd;
    char buf[1024];
    ssize_t len;
    struct stat st;
    const char *data = "symlink test data\n";

    printf("Testing symlink() syscall\n");
    fflush(stdout);

    // Setup: create target file
    fd = open(TEST_FILE, O_CREAT | O_WRONLY, 0644);
    if (fd == -1) { perror("open target"); exit(EXIT_FAILURE); }
    if (write(fd, data, strlen(data)) == -1) { perror("write target"); close(fd); exit(EXIT_FAILURE); }
    close(fd);

    // Test 1: symlink() creates a symbolic link
    printf("\n=== Test 1: symlink() ===\n");
    if (symlink(TEST_FILE, SYMLINK_FILE) == -1) {
        perror("symlink"); cleanup(); exit(EXIT_FAILURE);
    }
    printf("symlink() succeeded\n");

    // Test 2: readlink() returns correct target
    printf("\n=== Test 2: readlink() returns correct target ===\n");
    len = readlink(SYMLINK_FILE, buf, sizeof(buf) - 1);
    if (len == -1) { perror("readlink"); cleanup(); exit(EXIT_FAILURE); }
    buf[len] = '\0';
    if (strcmp(buf, TEST_FILE) != 0) {
        fprintf(stderr, "Error: expected '%s', got '%s'\n", TEST_FILE, buf);
        cleanup(); exit(EXIT_FAILURE);
    }
    printf("readlink() returned correct target: %s\n", buf);

    // Test 3: lstat() identifies symlink type
    printf("\n=== Test 3: lstat() identifies symlink ===\n");
    if (lstat(SYMLINK_FILE, &st) == -1) { perror("lstat"); cleanup(); exit(EXIT_FAILURE); }
    if (!S_ISLNK(st.st_mode)) {
        fprintf(stderr, "Error: expected symlink mode, got %o\n", st.st_mode);
        cleanup(); exit(EXIT_FAILURE);
    }
    printf("lstat() correctly identifies symlink\n");

    // Test 4: symlink() fails with EEXIST on duplicate
    printf("\n=== Test 4: EEXIST on duplicate symlink ===\n");
    if (symlink(TEST_FILE, SYMLINK_FILE) != -1) {
        fprintf(stderr, "Error: expected EEXIST\n");
        cleanup(); exit(EXIT_FAILURE);
    }
    if (errno != EEXIST) {
        fprintf(stderr, "Error: expected EEXIST, got errno %d\n", errno);
        cleanup(); exit(EXIT_FAILURE);
    }
    printf("correctly got EEXIST\n");

    // Test 5: symlinkat() with AT_FDCWD
    printf("\n=== Test 5: symlinkat() with AT_FDCWD ===\n");
    if (symlinkat(TEST_FILE, AT_FDCWD, SYMLINK_AT_FILE) == -1) {
        perror("symlinkat"); cleanup(); exit(EXIT_FAILURE);
    }
    len = readlink(SYMLINK_AT_FILE, buf, sizeof(buf) - 1);
    if (len == -1) { perror("readlink symlinkat"); cleanup(); exit(EXIT_FAILURE); }
    buf[len] = '\0';
    if (strcmp(buf, TEST_FILE) != 0) {
        fprintf(stderr, "Error: expected '%s', got '%s'\n", TEST_FILE, buf);
        cleanup(); exit(EXIT_FAILURE);
    }
    printf("symlinkat() correctly created symlink: %s\n", buf);

    cleanup();
    printf("\nAll symlink() tests passed successfully\n");
    fflush(stdout);
    return EXIT_SUCCESS;
}