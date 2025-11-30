#include <unistd.h>
#include <string.h>
#include <fcntl.h>
#include <sys/uio.h>
#include <sys/stat.h>
#include <stdio.h>
#include <stdlib.h>

int main() {
    const char* filename = "testfiles/writev_test.txt";
    int fd;
    ssize_t bytes_written, bytes_read;
    char read_buffer[256] = {0};
    
    // Create testfiles directory if it doesn't exist
    mkdir("testfiles", 0755);
        
    // Open file for writing
    fd = open(filename, O_RDWR | O_CREAT | O_TRUNC, 0777); 
    if (fd == -1) {
        perror("open failed");
        return 1;
    }
    
    // Write multiple buffers with writev
    char *buf1 = "Hello ";
    char *buf2 = "world ";
    char *buf3 = "from ";
    char *buf4 = "writev!\n";
    
    struct iovec iov[4];
    iov[0].iov_base = buf1;
    iov[0].iov_len = strlen(buf1);
    iov[1].iov_base = buf2;
    iov[1].iov_len = strlen(buf2);
    iov[2].iov_base = buf3;
    iov[2].iov_len = strlen(buf3);
    iov[3].iov_base = buf4;
    iov[3].iov_len = strlen(buf4);
    
    bytes_written = writev(fd, iov, 4);
    if (bytes_written == -1) {
        perror("writev failed");
        close(fd);
        return 1;
    }
    
    printf("writev wrote %zd bytes\n", (long)bytes_written);
    
    // Verify by reading back
    lseek(fd, 0, SEEK_SET);
    bytes_read = read(fd, read_buffer, sizeof(read_buffer) - 1);
    if (bytes_read == -1) {
        perror("read failed");
        close(fd);
        return 1;
    }
    
    read_buffer[bytes_read] = '\0';
    printf("Read back: %s", read_buffer);
    
    // Verify content matches
    const char* expected = "Hello world from writev!\n";
    if (strcmp(read_buffer, expected) != 0) {
        printf("ERROR: Content mismatch!\n");
        printf("Expected: %s", expected);
        printf("Got: %s", read_buffer);
        close(fd);
        return 1;
    }
    
    // Clean up
    if (close(fd) == -1) {
        perror("close failed");
        return 1;
    }
    
    return 0;
}

