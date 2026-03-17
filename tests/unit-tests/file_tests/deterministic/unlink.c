#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

#define TEST_FILE "testfiles/unlink_test_file.txt"
#define TEST_DIR "testfiles/unlink_test_dir"

int main() {
    int fd;
    struct stat st;
    
    printf("Testing unlink() syscall\n");
    fflush(stdout);
    
    // Test 1: Create and unlink a regular file
    fd = open(TEST_FILE, O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create test file");
        exit(EXIT_FAILURE);
    }
    
    // Write some data
    const char *data = "Test data for unlink testing\n";
    if (write(fd, data, strlen(data)) == -1) {
        perror("Failed to write to test file");
        close(fd);
        exit(EXIT_FAILURE);
    }
    close(fd);
    
    // Verify file exists
    if (stat(TEST_FILE, &st) == -1) {
        perror("Failed to stat test file before unlink");
        exit(EXIT_FAILURE);
    }
    
    // Unlink the file
    if (unlink(TEST_FILE) == -1) {
        perror("Failed to unlink test file");
        exit(EXIT_FAILURE);
    }
    
    // Verify file no longer exists
    if (stat(TEST_FILE, &st) != -1) {
        fprintf(stderr, "Error: File still exists after unlink\n");
        exit(EXIT_FAILURE);
    }
    
    if (errno != ENOENT) {
        fprintf(stderr, "Error: Expected ENOENT, got errno %d\n", errno);
        exit(EXIT_FAILURE);
    }
    
    // Test 2: Test error cases
    // Try to unlink non-existent file (should fail with ENOENT)
    if (unlink("nonexistent_file.txt") != -1) {
        fprintf(stderr, "Error: Should have failed to unlink non-existent file\n");
        exit(EXIT_FAILURE);
    }
    
    if (errno != ENOENT) {
        fprintf(stderr, "Error: Expected ENOENT, got errno %d\n", errno);
        exit(EXIT_FAILURE);
    }
    
    // Test 3: Test unlink with hard links
    // Create original file
    fd = open(TEST_FILE, O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create test file for hard link test");
        exit(EXIT_FAILURE);
    }
    close(fd);
    
    // Create hard link
    if (link(TEST_FILE, "unlink_test_link.txt") == -1) {
        perror("Failed to create hard link");
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify both files exist and have link count 2
    if (stat(TEST_FILE, &st) == -1) {
        perror("Failed to stat original file");
        unlink(TEST_FILE);
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    if (st.st_nlink != 2) {
        fprintf(stderr, "Error: Expected 2 hard links, got %ld\n", st.st_nlink);
        unlink(TEST_FILE);
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    // Unlink one of the files
    if (unlink(TEST_FILE) == -1) {
        perror("Failed to unlink original file");
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    // Verify the other file still exists with link count 1
    if (stat("unlink_test_link.txt", &st) == -1) {
        perror("Failed to stat remaining file after unlink");
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    if (st.st_nlink != 1) {
        fprintf(stderr, "Error: Expected 1 hard link after unlink, got %ld\n", st.st_nlink);
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    // Test 4: Test unlink on directory (should fail with EISDIR)
    if (mkdir(TEST_DIR, 0755) == -1) {
        perror("Failed to create test directory");
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    if (unlink(TEST_DIR) != -1) {
        fprintf(stderr, "Error: Should have failed to unlink directory\n");
        rmdir(TEST_DIR);
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    if (errno != EISDIR) {
        fprintf(stderr, "Error: Expected EISDIR, got errno %d\n", errno);
        rmdir(TEST_DIR);
        unlink("unlink_test_link.txt");
        exit(EXIT_FAILURE);
    }
    
    // Cleanup
    rmdir(TEST_DIR);
    unlink("unlink_test_link.txt");
    
    printf("All unlink() tests passed successfully\n");
    fflush(stdout);
    
    return EXIT_SUCCESS;
}
