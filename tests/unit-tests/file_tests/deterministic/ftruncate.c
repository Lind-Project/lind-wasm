#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

#define TEST_FILE "testfiles/ftruncate_test_file.txt"
#define INITIAL_SIZE 100
#define TRUNCATE_SIZE 50
#define EXPAND_SIZE 200

int main() {
    int fd;
    struct stat st;
    char buffer[256];
    ssize_t bytes_read;
    
    printf("Testing ftruncate() syscall\n");
    fflush(stdout);
    
    // Test 1: Create file and truncate to smaller size
    fd = open(TEST_FILE, O_CREAT | O_RDWR, 0644);
    if (fd == -1) {
        perror("Failed to create test file");
        exit(EXIT_FAILURE);
    }
    
    // Write initial data
    for (int i = 0; i < INITIAL_SIZE; i++) {
        if (write(fd, "A", 1) == -1) {
            perror("Failed to write initial data");
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Verify initial size
    if (fstat(fd, &st) == -1) {
        perror("Failed to stat file after initial write");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (st.st_size != INITIAL_SIZE) {
        fprintf(stderr, "Error: Expected size %d, got %ld\n", INITIAL_SIZE, st.st_size);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Truncate to smaller size
    if (ftruncate(fd, TRUNCATE_SIZE) == -1) {
        perror("Failed to truncate file to smaller size");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify truncated size
    if (fstat(fd, &st) == -1) {
        perror("Failed to stat file after truncate");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (st.st_size != TRUNCATE_SIZE) {
        fprintf(stderr, "Error: Expected truncated size %d, got %ld\n", TRUNCATE_SIZE, st.st_size);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify content is preserved up to truncate point
    if (lseek(fd, 0, SEEK_SET) == -1) {
        perror("Failed to seek to beginning");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    bytes_read = read(fd, buffer, TRUNCATE_SIZE);
    if (bytes_read != TRUNCATE_SIZE) {
        fprintf(stderr, "Error: Expected to read %d bytes, got %zd\n", TRUNCATE_SIZE, bytes_read);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify all bytes are 'A'
    for (int i = 0; i < TRUNCATE_SIZE; i++) {
        if (buffer[i] != 'A') {
            fprintf(stderr, "Error: Expected 'A' at position %d, got '%c'\n", i, buffer[i]);
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Test 2: Truncate to larger size (should extend file with zeros)
    if (ftruncate(fd, EXPAND_SIZE) == -1) {
        perror("Failed to truncate file to larger size");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify expanded size
    if (fstat(fd, &st) == -1) {
        perror("Failed to stat file after expansion");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (st.st_size != EXPAND_SIZE) {
        fprintf(stderr, "Error: Expected expanded size %d, got %ld\n", EXPAND_SIZE, st.st_size);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify original content is preserved
    if (lseek(fd, 0, SEEK_SET) == -1) {
        perror("Failed to seek to beginning after expansion");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    bytes_read = read(fd, buffer, TRUNCATE_SIZE);
    if (bytes_read != TRUNCATE_SIZE) {
        fprintf(stderr, "Error: Expected to read %d bytes after expansion, got %zd\n", TRUNCATE_SIZE, bytes_read);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify original content is still there
    for (int i = 0; i < TRUNCATE_SIZE; i++) {
        if (buffer[i] != 'A') {
            fprintf(stderr, "Error: Original content corrupted after expansion at position %d\n", i);
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Verify new area is zero-filled
    if (lseek(fd, TRUNCATE_SIZE, SEEK_SET) == -1) {
        perror("Failed to seek to expansion area");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    bytes_read = read(fd, buffer, EXPAND_SIZE - TRUNCATE_SIZE);
    if (bytes_read != (EXPAND_SIZE - TRUNCATE_SIZE)) {
        fprintf(stderr, "Error: Expected to read %d bytes from expansion area, got %zd\n", 
                EXPAND_SIZE - TRUNCATE_SIZE, bytes_read);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify expansion area is zero-filled
    for (int i = 0; i < (EXPAND_SIZE - TRUNCATE_SIZE); i++) {
        if (buffer[i] != '\0') {
            fprintf(stderr, "Error: Expected zero at expansion position %d, got 0x%02x\n", i, (unsigned char)buffer[i]);
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Test 3: Truncate to zero size
    if (ftruncate(fd, 0) == -1) {
        perror("Failed to truncate file to zero size");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify zero size
    if (fstat(fd, &st) == -1) {
        perror("Failed to stat file after zero truncate");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (st.st_size != 0) {
        fprintf(stderr, "Error: Expected zero size, got %ld\n", st.st_size);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 4: Test error cases
    // Try to truncate with negative size (should fail with EINVAL)
    if (ftruncate(fd, -1) != -1) {
        fprintf(stderr, "Error: Should have failed to truncate with negative size\n");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (errno != EINVAL) {
        fprintf(stderr, "Error: Expected EINVAL, got errno %d\n", errno);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 5: Test with read-only file descriptor
    close(fd);
    fd = open(TEST_FILE, O_RDONLY);
    if (fd == -1) {
        perror("Failed to open file read-only");
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (ftruncate(fd, 10) != -1) {
        fprintf(stderr, "Error: Should have failed to truncate read-only file\n");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (errno != EBADF && errno != EINVAL) {
        fprintf(stderr, "Error: Expected EBADF or EINVAL, got errno %d\n", errno);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Cleanup
    close(fd);
    unlink(TEST_FILE);
    
    printf("All ftruncate() tests passed successfully\n");
    fflush(stdout);
    
    return EXIT_SUCCESS;
}
