/* Tests for fstatat.
 *
 * Catches fstatat being a silent 0 with an unfilled buffer (the
 * centerpiece of #1173): fstatat must populate buf with file metadata
 * matching what stat() reports.
 *
 * Symlink semantics for fstatat (AT_SYMLINK_NOFOLLOW vs follow) and
 * lstat are already covered by tests/.../file_tests/deterministic/lstat.c,
 * which now exercises the new NEWFSTATAT-routed lstat plumbing under
 * the hood.  Dirfd resolution is covered by openat.c / readlinkat.c.
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define FILE_PATH "testfiles/fstatat-target"

static const char content[] = "hello fstatat";

int main(void)
{
    int fd = open(FILE_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    assert(write(fd, content, sizeof(content) - 1) == sizeof(content) - 1);
    close(fd);

    /* fstatat(AT_FDCWD, ...) must populate buf — pre-fix this returned
     * 0 with the buffer untouched.  Pre-fill with 0xff so a no-op is
     * detectable. */
    struct stat st;
    memset(&st, 0xff, sizeof(st));
    assert(fstatat(AT_FDCWD, FILE_PATH, &st, 0) == 0);
    assert(st.st_size == (off_t)(sizeof(content) - 1));
    assert(S_ISREG(st.st_mode));

    /* Cross-check against stat() — same file, must agree on size and
     * type. */
    struct stat st_stat;
    assert(stat(FILE_PATH, &st_stat) == 0);
    assert(st.st_size == st_stat.st_size);
    assert((st.st_mode & S_IFMT) == (st_stat.st_mode & S_IFMT));

    unlink(FILE_PATH);
    return 0;
}
