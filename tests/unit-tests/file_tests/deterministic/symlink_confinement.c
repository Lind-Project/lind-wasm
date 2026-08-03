#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main() {
    unlink("evil_link");

    assert(symlink("/etc/passwd", "evil_link") == 0);

    errno = 0;
    int direct_fd = open("/etc/passwd", O_RDONLY);
    int direct_errno = errno;

    errno = 0;
    int link_fd = open("evil_link", O_RDONLY);
    int link_errno = errno;

    if(direct_fd == -1) {
        assert(link_fd == -1);
        assert(link_errno == direct_errno);
    } else {
        assert(link_fd != -1);
        
        char direct_buf[256];
        char link_buf[256];
        ssize_t direct_n = read(direct_fd, direct_buf, sizeof(direct_buf));
        ssize_t link_n = read(link_fd, link_buf, sizeof(link_buf));

        assert(direct_n >= 0);
        assert(link_n == direct_n);
        assert(memcmp(direct_buf, link_buf, (size_t)direct_n) == 0);

        close(direct_fd);
        close(link_fd);
    }

    unlink("evil_link");

    printf("symlink_confinement test: PASS\n");
    return 0;
    
}