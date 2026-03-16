#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(void) {
    const char *filename = "read_test.txt";
    const size_t payload_size = 4096;
    char write_buf[4096];
    char read_buf[4096] = {0};
    
    // Create a fixed payload: repeating "ABCD..." pattern
    for (size_t i = 0; i < payload_size; i++) {
        write_buf[i] = 'A' + (i % 26);
    }
    
    // Create a local file "read_test.txt" with O_CREAT|O_TRUNC|O_WRONLY, mode 0644
    int fd = open(filename, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    assert(fd >= 0);
    
    // Write the fixed payload
    ssize_t written = write(fd, write_buf, payload_size);
    assert(written == (ssize_t)payload_size);
    
    // Close
    assert(close(fd) == 0);
    
    // Reopen O_RDONLY
    fd = open(filename, O_RDONLY);
    assert(fd >= 0);
    
    // Read exactly 4096 bytes into buffer (loop until full)
    size_t total_read = 0;
    while (total_read < payload_size) {
        ssize_t ret = read(fd, read_buf + total_read, payload_size - total_read);
        assert(ret > 0);
        total_read += ret;
    }
    assert(total_read == payload_size);
    
    // Assert content matches expected (memcmp with the original payload buffer)
    assert(memcmp(read_buf, write_buf, payload_size) == 0);
    
    // Close fd and return
    assert(close(fd) == 0);
    
    return 0;
}
