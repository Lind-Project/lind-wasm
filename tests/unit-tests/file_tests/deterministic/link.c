#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

#define TEST_FILE "testfiles/link_test_file.txt"
#define LINK_FILE "testfiles/link_test_link.txt"

int main() {
    int fd;
    struct stat stat_orig, stat_link;
    
    printf("Testing link() syscall\n");
    fflush(stdout);
    
    // Create original file
    fd = open(TEST_FILE, O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create test file");
        exit(EXIT_FAILURE);
    }
    
    // Write some data to the file
    const char *data = "This is test data for link testing\n";
    if (write(fd, data, strlen(data)) == -1) {
        perror("Failed to write to test file");
        close(fd);
        exit(EXIT_FAILURE);
    }
    close(fd);
    
    // Test 1: Create a hard link
    if (link(TEST_FILE, LINK_FILE) == -1) {
        perror("Failed to create hard link");
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 2: Verify both files exist and have same inode
    if (stat(TEST_FILE, &stat_orig) == -1) {
        perror("Failed to stat original file");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (stat(LINK_FILE, &stat_link) == -1) {
        perror("Failed to stat link file");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (stat_orig.st_ino != stat_link.st_ino) {
        fprintf(stderr, "Error: Original and link files have different inodes\n");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (stat_orig.st_nlink != 2) {
        fprintf(stderr, "Error: Expected 2 hard links, got %u\n", stat_orig.st_nlink);
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 3: Verify both files have same content
    char buffer1[256], buffer2[256];
    fd = open(TEST_FILE, O_RDONLY);
    if (fd == -1) {
        perror("Failed to open original file for reading");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    ssize_t bytes1 = read(fd, buffer1, sizeof(buffer1) - 1);
    close(fd);
    
    fd = open(LINK_FILE, O_RDONLY);
    if (fd == -1) {
        perror("Failed to open link file for reading");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    ssize_t bytes2 = read(fd, buffer2, sizeof(buffer2) - 1);
    close(fd);
    
    if (bytes1 != bytes2 || memcmp(buffer1, buffer2, bytes1) != 0) {
        fprintf(stderr, "Error: Original and link files have different content\n");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 4: Test error cases
    // Try to link to existing file (should fail with EEXIST)
    if (link(TEST_FILE, LINK_FILE) != -1) {
        fprintf(stderr, "Error: Should have failed to create duplicate link\n");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (errno != EEXIST) {
        fprintf(stderr, "Error: Expected EEXIST, got errno %d\n", errno);
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Try to link to non-existent file (should fail with ENOENT)
    if (link("nonexistent_file.txt", "new_link.txt") != -1) {
        fprintf(stderr, "Error: Should have failed to link non-existent file\n");
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (errno != ENOENT) {
        fprintf(stderr, "Error: Expected ENOENT, got errno %d\n", errno);
        unlink(TEST_FILE);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 5: Verify link count decreases when one file is deleted
    if (unlink(TEST_FILE) == -1) {
        perror("Failed to unlink original file");
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (stat(LINK_FILE, &stat_link) == -1) {
        perror("Failed to stat link file after original deletion");
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (stat_link.st_nlink != 1) {
        fprintf(stderr, "Error: Expected 1 hard link after deletion, got %u\n", stat_link.st_nlink);
        unlink(LINK_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Cleanup
    unlink(LINK_FILE);
    
    printf("All link() tests passed successfully\n");
    fflush(stdout);
    
    return EXIT_SUCCESS;
}
