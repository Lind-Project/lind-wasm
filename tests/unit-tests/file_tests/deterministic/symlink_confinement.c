#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main() {
    unlink("evil_link");

    assert(symlink("/etc/passwd", "evil_link") == 0);

    /* 
    NOTE: this only checks that symlink resolution is internally consistent
    (following evil_link behaves like a normal read), not that either path
    is actually confined. See the sentinel-file checks below for actual 
    confinement/escape proof.
    */
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

    errno = 0;
    int sentinel_fd = open("/tmp/lind/sentinel.txt", O_RDONLY);

    if(sentinel_fd == -1) {
        fprintf(stderr, "symlink_confinement test: FAIL - could not open "
                "/tmp/lind/sentinel.txt from inside the cage (errno %d)\n", errno);
        assert(0);
    }

    char sentinel_buf[64] = {0};
    ssize_t sentinel_n = read(sentinel_fd, sentinel_buf, sizeof(sentinel_buf) - 1);
    close(sentinel_fd);

    assert(sentinel_n >= 0);
    sentinel_buf[sentinel_n] = '\0';

    if(strcmp(sentinel_buf, "LIND_HOST_ONLY") == 0) {
        fprintf(stderr, "symlink_confinement test: FAIL - read host sentinel "
                "from inside the cage, chroot escape\n");
        assert(0);
    } else if(strcmp(sentinel_buf, "LIND_CAGE_ONLY") != 0) {
        fprintf(stderr, "symlink_confinement test: FAIL - unexpected sentinel "
                "content: \"%s\"\n", sentinel_buf);
        assert(0);
    }
    unlink("evil_link_sentinel");
    assert(symlink("/tmp/lind/sentinel.txt", "evil_link_sentinel") == 0);

    errno = 0;
    int link_sentinel_fd = open("evil_link_sentinel", O_RDONLY);
    if(link_sentinel_fd == -1) {
        fprintf(stderr, "symlink_confinement test: FAIL - could not open "
                "evil_link sentinel from inside the cage (errno %d)\n", errno);
        assert(0);
    }

    char link_sentinel_buf[64] = {0};
    ssize_t link_sentinel_n = read(link_sentinel_fd, link_sentinel_buf, sizeof(link_sentinel_buf) - 1);
    close(link_sentinel_fd);

    assert(link_sentinel_n >= 0);
    link_sentinel_buf[link_sentinel_n] = '\0';

    if(strcmp(link_sentinel_buf, "LIND_HOST_ONLY") == 0) {
        fprintf(stderr, "symlink_confinement test: FAIL - read host sentinel "
                "via symlink from inside the cage, chroot escape via symlink target\n");
        assert(0);
    } else if(strcmp(link_sentinel_buf, "LIND_CAGE_ONLY") != 0) {
        fprintf(stderr, "symlink_confinement test: FAIL - unexpected sentinel "
                "content via symlink: \"%s\"\n", link_sentinel_buf);
        assert(0);
    }

    unlink("evil_link_sentinel");

    printf("symlink_confinement test: PASS\n");
    return 0;
}
