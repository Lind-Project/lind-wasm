/* Tests for faccessat.
 *
 * Catches faccessat being a silent 0-return when the file actually
 * isn't accessible. With fchmod we lock down the file's mode, then
 * probe via faccessat — a pre-fix faccessat returns 0 regardless,
 * which the assertions catch.
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#define DIR_PATH  "testfiles/faccessat-dir"
#define FILE_NAME "f"
#define FILE_PATH DIR_PATH "/" FILE_NAME

int main(void)
{
    mkdir(DIR_PATH, 0755);
    int fd = open(FILE_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    close(fd);

    /* 1. F_OK on existing file: success. */
    assert(faccessat(AT_FDCWD, FILE_PATH, F_OK, 0) == 0);

    /* 2. F_OK on missing file: -1, ENOENT. */
    assert(faccessat(AT_FDCWD, DIR_PATH "/missing", F_OK, 0) == -1);
    assert(errno == ENOENT);

    /* 3. R_OK on a readable file. */
    assert(faccessat(AT_FDCWD, FILE_PATH, R_OK, 0) == 0);

    /* 4. faccessat with a real dirfd. */
    int dirfd = open(DIR_PATH, O_RDONLY | O_DIRECTORY);
    assert(dirfd >= 0);
    assert(faccessat(dirfd, FILE_NAME, F_OK, 0) == 0);
    errno = 0;
    assert(faccessat(dirfd, "missing", F_OK, 0) == -1);
    assert(errno == ENOENT);
    close(dirfd);

    unlink(FILE_PATH);
    rmdir(DIR_PATH);
    return 0;
}
