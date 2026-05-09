/* Tests for fchmodat.
 *
 * Catches fchmodat being a silent no-op (the bug fixed in this PR):
 * a successful return with no actual mode change shows up as stat
 * reporting the original mode unchanged.
 *
 * Exercises:
 *   - fchmodat(AT_FDCWD, path, mode, 0) — the path-based form
 *   - fchmodat(dirfd, name, mode, 0)    — the directory-fd form
 *
 * Note: lchmod (which routes through fchmodat with AT_SYMLINK_NOFOLLOW)
 * is intentionally NOT tested here.  On Linux the kernel returns
 * EOPNOTSUPP for fchmodat(AT_SYMLINK_NOFOLLOW) on regular files unless
 * glibc's userspace O_PATH+/proc fallback is used, which this lind glibc
 * port doesn't include yet.
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#define FILE_PATH "testfiles/fchmodat-target"
#define DIR_PATH  "testfiles"
#define FILE_NAME "fchmodat-target"

int main(void)
{
    /* Setup. */
    int fd = open(FILE_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    close(fd);

    /* 1. fchmodat(AT_FDCWD, ...). */
    assert(fchmodat(AT_FDCWD, FILE_PATH, 0600, 0) == 0);
    struct stat st;
    assert(stat(FILE_PATH, &st) == 0);
    assert((st.st_mode & 07777) == 0600);

    /* 2. fchmodat with a real directory fd + relative name. */
    int dirfd = open(DIR_PATH, O_RDONLY | O_DIRECTORY);
    assert(dirfd >= 0);
    assert(fchmodat(dirfd, FILE_NAME, 0700, 0) == 0);
    assert(stat(FILE_PATH, &st) == 0);
    assert((st.st_mode & 07777) == 0700);
    close(dirfd);

    unlink(FILE_PATH);
    return 0;
}
