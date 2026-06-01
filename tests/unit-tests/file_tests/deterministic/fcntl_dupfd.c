#include <fcntl.h>
#include <unistd.h>
#include <assert.h>

int main() {
    int fd = open("/tmp/fcntl_test", O_CREAT | O_RDWR, 0644);
    assert(fd >= 0 && "open failed");

    // F_DUPFD with minfd=10 - before fix this returned EBADF
    // because glibc added wasm base address to the integer arg
    int newfd = fcntl(fd, F_DUPFD, 10);
    assert(newfd >= 0 && "F_DUPFD failed - likely integer arg corruption");
    assert(newfd >= 10 && "F_DUPFD returned fd below minimum");

    // F_SETFD with FD_CLOEXEC - also takes integer arg
    int ret = fcntl(newfd, F_SETFD, FD_CLOEXEC);
    assert(ret == 0 && "F_SETFD failed");

    // Verify the flag was set
    int flags = fcntl(newfd, F_GETFD);
    assert(flags >= 0 && "F_GETFD failed");
    assert((flags & FD_CLOEXEC) && "FD_CLOEXEC not set");

    close(fd);
    close(newfd);
    unlink("/tmp/fcntl_test");
    return 0;
}
