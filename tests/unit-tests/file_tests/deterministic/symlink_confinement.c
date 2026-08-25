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
    is actually confined. See the /lind/README.md-based checks below for
    the actual confinement/escape proof.
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
    /*
    NOTE: /lind/README.md is specific to this dev-conatainer's mount
    layout(repo checked out at /lind, matching LINDFS_ROOT's hardcoded
    assumption in sysdefs). If this runs somewhere that mounts the repo 
    differently, this path may not exist at all, in which case this 
    check would accidentally pass via ENOENT.
    Re-verify this path is valid if test is run in a new environment.
    */
    int escape_fd = open("/lind/README.md", O_RDONLY);
    int escape_errno = errno;

    if(escape_fd != -1) {
        fprintf(stderr, "symlink_confinement test: FAIL -- opened "
                "/lind/README.md from inside the cage, chroot escape\n");
        close(escape_fd);
        assert(0);
    }
    assert(escape_errno == ENOENT);

    unlink("evil_link_readme");
    assert(symlink("/lind/README.md", "evil_link_readme") == 0);

    errno = 0;
    int link_escape_fd = open("evil_link_readme", O_RDONLY);
    int link_escape_errno = errno;
    if(link_escape_fd != -1) {
        fprintf(stderr, "symlink_confinement test: FAIL -- opened "
              "/lind/README.md via symlink from inside the cage, "
              "chroot escape via symlink target\n");
        close(link_escape_fd);
        assert(0);
    }
    assert(link_escape_errno == ENOENT);
    unlink("evil_link_readme");

    printf("symlink_confinement test: PASS\n");
    return 0;
}
