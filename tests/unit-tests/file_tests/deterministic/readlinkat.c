#include <unistd.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>
#include <assert.h>

const char* VALID_SYMBOLIC_PATH = "testfiles/readlinkfile";
const char* NON_SYMBOLIC_PATH = "testfiles/fstatfile.txt";
const char* NON_EXISTENT_PATH = "testfiles/nonexistent";

void test_readlinkat() {
    char buf[1024];
    ssize_t len;

    // Test Case 1: Valid symbolic link with AT_FDCWD
    len = readlinkat(AT_FDCWD, VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    assert(len != -1 && "Test Case 1: readlinkat should succeed");
    buf[len] = '\0';
    assert(strcmp(buf, "readlinkfile.txt") == 0 && "Test Case 1: wrong symlink target");
    printf("Test Case 1: PASS\n");

    // Test Case 2: Valid symbolic link with a file descriptor
    int dirfd = open("testfiles/", O_RDONLY);
    assert(dirfd != -1 && "Failed to open directory");

    len = readlinkat(dirfd, VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    assert(len != -1 && "Test Case 2: readlinkat should succeed");                                                                    
    buf[len] = '\0';                                                                                                                  
    assert(strcmp(buf, "readlinkfile.txt") == 0 && "Test Case 2: wrong symlink target");                                              
    printf("Test Case 2: PASS\n");                                                                                                    
    close(dirfd);  

    // Test Case 3: Non-existent symbolic link
    len = readlinkat(AT_FDCWD, NON_EXISTENT_PATH, buf, sizeof(buf));
    assert(len == -1 && errno == ENOENT && "Test Case 3: should fail with ENOENT");                                                   
    printf("Test Case 3: PASS\n"); 

    // Test Case 4: Invalid file descriptor
    len = readlinkat(-1, VALID_SYMBOLIC_PATH, buf, sizeof(buf));
    assert(len == -1 && (errno == EBADF || errno == EINVAL) && "Test Case 4: should fail with EBADF/EINVAL");                         
    printf("Test Case 4: PASS\n");
}

int main() {
    test_readlinkat();
    return 0;
}
