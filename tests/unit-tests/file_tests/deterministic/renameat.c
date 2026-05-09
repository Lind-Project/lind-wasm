/* Tests for renameat and renameat2.
 *
 * Catches renameat / renameat2 being silent 0-returns (the pre-fix
 * INLINE_SYSCALL_CALL no-op): a successful return means the rename
 * actually moved the file, which we verify with stat.
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <unistd.h>

#define DIR_PATH "testfiles/renameat-dir"
#define OLD_NAME "old"
#define NEW_NAME "new"
#define OLD_PATH DIR_PATH "/" OLD_NAME
#define NEW_PATH DIR_PATH "/" NEW_NAME

int main(void)
{
    mkdir(DIR_PATH, 0755);

    /* 1. renameat(AT_FDCWD, ..., AT_FDCWD, ...). */
    int fd = open(OLD_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    close(fd);

    assert(renameat(AT_FDCWD, OLD_PATH, AT_FDCWD, NEW_PATH) == 0);

    struct stat st;
    assert(stat(NEW_PATH, &st) == 0);
    errno = 0;
    assert(stat(OLD_PATH, &st) == -1);
    assert(errno == ENOENT);

    /* 2. renameat using real dirfd on both sides. */
    int dirfd = open(DIR_PATH, O_RDONLY | O_DIRECTORY);
    assert(dirfd >= 0);
    assert(renameat(dirfd, NEW_NAME, dirfd, OLD_NAME) == 0);
    assert(stat(OLD_PATH, &st) == 0);
    errno = 0;
    assert(stat(NEW_PATH, &st) == -1);
    assert(errno == ENOENT);
    close(dirfd);

    /* 3. renameat2 with flags=0 — same as renameat. */
    assert(renameat2(AT_FDCWD, OLD_PATH, AT_FDCWD, NEW_PATH, 0) == 0);
    assert(stat(NEW_PATH, &st) == 0);

    unlink(NEW_PATH);
    rmdir(DIR_PATH);
    return 0;
}
