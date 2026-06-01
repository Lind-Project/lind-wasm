#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <assert.h>

int main() {
    // Test 1: openat with AT_FDCWD (same as open)
    int fd1 = openat(AT_FDCWD, "testfiles/filetestfile.txt", O_RDONLY);
    assert(fd1 >= 0);
    printf("openat AT_FDCWD succeeded with fd = %d\n", fd1);
    close(fd1);

    // Test 2: openat with directory fd
    int dirfd = open("testfiles", O_RDONLY | O_DIRECTORY);
    assert(dirfd >= 0);

    int fd2 = openat(dirfd, "filetestfile.txt", O_RDONLY);
    assert(fd2 >= 0);
    printf("openat with dirfd succeeded with fd = %d\n", fd2);

    close(fd2);
    close(dirfd);

    // Test 3: openat with invalid fd should fail
    int fd3 = openat(9999, "filetestfile.txt", O_RDONLY);
    assert(fd3 == -1);
    printf("openat with invalid fd correctly failed\n");

    printf("All openat tests passed\n");
    return 0;
}
