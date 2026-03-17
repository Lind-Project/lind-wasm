#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <fcntl.h>
#include <string.h>

#define TEST_FILE "testfiles/lseek_test_file.txt"
#define FILE_SIZE 100

int main() {
    int fd;
    off_t pos;
    char buffer[256];
    ssize_t bytes_written, bytes_read;
    
    printf("Testing lseek() syscall\n");
    fflush(stdout);
    
    // Create test file
    fd = open(TEST_FILE, O_CREAT | O_RDWR, 0644);
    if (fd == -1) {
        perror("Failed to create test file");
        exit(EXIT_FAILURE);
    }
    
    // Write test data
    for (int i = 0; i < FILE_SIZE; i++) {
        char c = 'A' + (i % 26);
        if (write(fd, &c, 1) == -1) {
            perror("Failed to write test data");
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Test 1: SEEK_SET - seek to absolute position
    pos = lseek(fd, 10, SEEK_SET);
    if (pos == -1) {
        perror("Failed to seek to position 10");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (pos != 10) {
        fprintf(stderr, "Error: Expected position 10, got %ld\n", pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Read from position 10
    bytes_read = read(fd, buffer, 5);
    if (bytes_read != 5) {
        fprintf(stderr, "Error: Expected to read 5 bytes, got %zd\n", bytes_read);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify content (should be 'K' to 'O')
    for (int i = 0; i < 5; i++) {
        char expected = 'A' + ((10 + i) % 26);
        if (buffer[i] != expected) {
            fprintf(stderr, "Error: Expected '%c' at position %d, got '%c'\n", 
                    expected, 10 + i, buffer[i]);
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Test 2: SEEK_CUR - seek relative to current position
    pos = lseek(fd, 5, SEEK_CUR);
    if (pos == -1) {
        perror("Failed to seek 5 bytes forward");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (pos != 20) {
        fprintf(stderr, "Error: Expected position 20, got %ld\n", pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 3: SEEK_END - seek relative to end of file
    pos = lseek(fd, -10, SEEK_END);
    if (pos == -1) {
        perror("Failed to seek 10 bytes from end");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (pos != (FILE_SIZE - 10)) {
        fprintf(stderr, "Error: Expected position %d, got %ld\n", FILE_SIZE - 10, pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Read from near end
    bytes_read = read(fd, buffer, 10);
    if (bytes_read != 10) {
        fprintf(stderr, "Error: Expected to read 10 bytes from end, got %zd\n", bytes_read);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Verify content (should be last 10 characters)
    for (int i = 0; i < 10; i++) {
        char expected = 'A' + ((FILE_SIZE - 10 + i) % 26);
        if (buffer[i] != expected) {
            fprintf(stderr, "Error: Expected '%c' at end position %d, got '%c'\n", 
                    expected, FILE_SIZE - 10 + i, buffer[i]);
            close(fd);
            unlink(TEST_FILE);
            exit(EXIT_FAILURE);
        }
    }
    
    // Test 4: Seek beyond end of file and write (should extend file)
    pos = lseek(fd, 20, SEEK_END);
    if (pos == -1) {
        perror("Failed to seek beyond end of file");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (pos != (FILE_SIZE + 20)) {
        fprintf(stderr, "Error: Expected position %d, got %ld\n", FILE_SIZE + 20, pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Write data beyond end
    const char *extend_data = "EXTENDED";
    bytes_written = write(fd, extend_data, strlen(extend_data));
    if (bytes_written != strlen(extend_data)) {
        fprintf(stderr, "Error: Expected to write %zu bytes, got %zd\n", 
                strlen(extend_data), bytes_written);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Seek back and verify the data
    pos = lseek(fd, FILE_SIZE + 20, SEEK_SET);
    if (pos == -1) {
        perror("Failed to seek back to extended area");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    bytes_read = read(fd, buffer, strlen(extend_data));
    if (bytes_read != strlen(extend_data)) {
        fprintf(stderr, "Error: Expected to read %zu bytes from extended area, got %zd\n", 
                strlen(extend_data), bytes_read);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (memcmp(buffer, extend_data, strlen(extend_data)) != 0) {
        fprintf(stderr, "Error: Extended data doesn't match\n");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 5: Test error cases
    // Try to seek with invalid whence
    pos = lseek(fd, 0, 999);
    if (pos != -1) {
        fprintf(stderr, "Error: Should have failed with invalid whence\n");
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
    
    // Test 6: Seek on read-only file descriptor
    close(fd);
    fd = open(TEST_FILE, O_RDONLY);
    if (fd == -1) {
        perror("Failed to open file read-only");
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Seek should work on read-only file
    pos = lseek(fd, 50, SEEK_SET);
    if (pos == -1) {
        perror("Failed to seek on read-only file");
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    if (pos != 50) {
        fprintf(stderr, "Error: Expected position 50 on read-only file, got %ld\n", pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Test 7: Seek to beginning and end
    pos = lseek(fd, 0, SEEK_SET);
    if (pos != 0) {
        fprintf(stderr, "Error: Expected position 0 at beginning, got %ld\n", pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    pos = lseek(fd, 0, SEEK_END);
    if (pos != (FILE_SIZE + 20 + strlen(extend_data))) {
        fprintf(stderr, "Error: Expected position %ld at end, got %ld\n", 
                FILE_SIZE + 20 + strlen(extend_data), pos);
        close(fd);
        unlink(TEST_FILE);
        exit(EXIT_FAILURE);
    }
    
    // Cleanup
    close(fd);
    unlink(TEST_FILE);
    
    printf("All lseek() tests passed successfully\n");
    fflush(stdout);
    
    return EXIT_SUCCESS;
}
