#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <unistd.h>

#define TEST_FILE "testfiles/mknod_test_file"
#define TEST_FIFO "testfiles/mknod_test_fifo"

int main() {
    struct stat st;

    // Test 1: Create regular file with mknod
    if (mknod(TEST_FILE, S_IFREG | 0644, 0) == -1) {
        perror("mknod regular file");
        exit(EXIT_FAILURE);
    }
    if (stat(TEST_FILE, &st) == -1 || !S_ISREG(st.st_mode)) {
        fprintf(stderr, "Error: mknod didn't create a regular file\n");
        exit(EXIT_FAILURE);
    }
    printf("Created regular file successfully\n");
    fflush(stdout);

    // Test 2: Create FIFO with mknod
    if (mknod(TEST_FIFO, S_IFIFO | 0644, 0) == -1) {
        perror("mknod fifo");
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    if (stat(TEST_FIFO, &st) == -1 || !S_ISFIFO(st.st_mode)) {
        fprintf(stderr, "Error: mknod did not create a FIFO\n");
        exit(EXIT_FAILURE);
    }
    printf("Created FIFO successfully\n");
    fflush(stdout);

    // Test 3: EEXIST - file already exists
    if (mknod(TEST_FILE, S_IFREG | 0644, 0) != -1) {
        fprintf(stderr, "Error: should have failed on existing file\n");
        exit(EXIT_FAILURE);
    }
    printf("EEXIST error handled correctly\n");
    fflush(stdout);

    // Test 4: ENOENT - parent directory doesn't exist
    if (mknod("nonexistent_dir/file", S_IFREG | 0644, 0) != -1) {
        fprintf(stderr, "Error: should have failed on bad path\n");
        exit(EXIT_FAILURE);
    }
    printf("ENOENT error handled correctly\n");
    fflush(stdout);

    // Cleanup
    unlink(TEST_FILE);
    unlink(TEST_FIFO);

    printf("All mknod tests passed successfully\n");
    fflush(stdout);
    return EXIT_SUCCESS;
}