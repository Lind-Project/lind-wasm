#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <sys/statfs.h>
#include <unistd.h>

int main() {
    // Open/create a test file
    int fd = open("fstatfs_test.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);

    // Call fstatfs
    struct statfs st = {0};
    assert(fstatfs(fd, &st) == 0);

    // Assert generic invariants that should hold across environments
    assert(st.f_bsize > 0);
#ifdef __USE_MISC
    // f_namelen is available on Linux (GNU extensions)
    assert(st.f_namelen > 0);
#endif

    // Close file
    assert(close(fd) == 0);

    // Clean up test file
    assert(unlink("fstatfs_test.txt") == 0);

    return 0;
}
