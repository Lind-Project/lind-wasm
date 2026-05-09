/* Tests for fstatat (and lstat now that it's correctly routed through
 * NEWFSTATAT_SYSCALL with AT_SYMLINK_NOFOLLOW).
 *
 * Catches:
 *   - fstatat being a silent 0 with an unfilled buffer (the centerpiece of #1173).
 *   - lstat following symlinks (the pre-fix bug in lstat64.c, which routed
 *     to XSTAT_SYSCALL with stat semantics).
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define DIR_PATH    "testfiles/fstatat-dir"
#define FILE_NAME   "f"
#define FILE_PATH   DIR_PATH "/" FILE_NAME
#define LINK_NAME   "lnk"
#define LINK_PATH   DIR_PATH "/" LINK_NAME

static const char content[] = "hello fstatat";

int main(void)
{
    /* Setup. */
    mkdir(DIR_PATH, 0755);
    int fd = open(FILE_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    assert(write(fd, content, sizeof(content) - 1) == sizeof(content) - 1);
    close(fd);

    /* 1. fstatat(AT_FDCWD, ...) — non-zero buf is the contract. */
    struct stat st;
    memset(&st, 0xff, sizeof(st));
    assert(fstatat(AT_FDCWD, FILE_PATH, &st, 0) == 0);
    assert(st.st_size == (off_t)(sizeof(content) - 1));
    assert(S_ISREG(st.st_mode));

    /* 2. fstatat with a real dirfd + relative name. */
    int dirfd = open(DIR_PATH, O_RDONLY | O_DIRECTORY);
    assert(dirfd >= 0);
    memset(&st, 0xff, sizeof(st));
    assert(fstatat(dirfd, FILE_NAME, &st, 0) == 0);
    assert(st.st_size == (off_t)(sizeof(content) - 1));
    close(dirfd);

    /* 3. fstatat AT_SYMLINK_NOFOLLOW vs follow on a symlink:
     *    follow → file size; nofollow → symlink size (= strlen(target)). */
    assert(symlink(FILE_NAME, LINK_PATH) == 0);

    struct stat st_follow, st_nofollow;
    assert(fstatat(AT_FDCWD, LINK_PATH, &st_follow, 0) == 0);
    assert(fstatat(AT_FDCWD, LINK_PATH, &st_nofollow, AT_SYMLINK_NOFOLLOW) == 0);
    assert(S_ISREG(st_follow.st_mode));
    assert(S_ISLNK(st_nofollow.st_mode));
    assert(st_follow.st_size != st_nofollow.st_size);

    /* 4. lstat — must equal the AT_SYMLINK_NOFOLLOW stat above. */
    struct stat st_lstat;
    assert(lstat(LINK_PATH, &st_lstat) == 0);
    assert(S_ISLNK(st_lstat.st_mode));
    assert(st_lstat.st_size == st_nofollow.st_size);

    unlink(LINK_PATH);
    unlink(FILE_PATH);
    rmdir(DIR_PATH);
    return 0;
}
