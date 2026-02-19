#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/statfs.h>

int main() {
    // Create a local file path "statfs_test.txt"
    int fd = open("statfs_test.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    assert(close(fd) == 0);

    // Call statfs on the file path
    struct statfs st = {0};
    assert(statfs("statfs_test.txt", &st) == 0);

    // Assert generic invariants that should always hold
    assert(st.f_bsize > 0);
    assert(st.f_blocks >= 0);

    // Clean up test file
    assert(unlink("statfs_test.txt") == 0);

    return 0;
}
