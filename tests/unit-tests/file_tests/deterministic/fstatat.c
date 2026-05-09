/* Tests for fstatat — minimal AT_FDCWD round-trip.
 *
 * Catches fstatat being a silent 0 with an unfilled buffer (the
 * centerpiece of #1173): fstatat must populate buf with file metadata
 * matching what stat() reports.
 *
 * Uses progress prints so the deterministic harness's native-vs-wasm
 * stdout comparison catches partial completion (e.g., wasm dies after
 * "setup ok" but before "fstatat ok").
 *
 * Symlink semantics for fstatat (AT_SYMLINK_NOFOLLOW vs follow) and
 * lstat are already covered by tests/.../file_tests/deterministic/lstat.c,
 * which now exercises the new NEWFSTATAT-routed lstat plumbing under
 * the hood.  Dirfd resolution is covered by openat.c / readlinkat.c.
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define FILE_PATH "testfiles/fstatat-target"

static const char content[] = "hello fstatat";

int main(void)
{
    int fd = open(FILE_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) { perror("open"); return 1; }
    if (write(fd, content, sizeof(content) - 1) != (ssize_t)(sizeof(content) - 1)) {
        perror("write"); return 1;
    }
    close(fd);
    printf("setup ok\n");

    struct stat st;
    memset(&st, 0, sizeof(st));
    if (fstatat(AT_FDCWD, FILE_PATH, &st, 0) != 0) {
        perror("fstatat"); return 1;
    }
    printf("fstatat ok\n");

    if ((size_t)st.st_size != sizeof(content) - 1) { printf("size wrong\n"); return 1; }
    if (!S_ISREG(st.st_mode)) { printf("not regular\n"); return 1; }
    printf("checks ok\n");

    struct stat st_stat;
    if (stat(FILE_PATH, &st_stat) != 0) { perror("stat"); return 1; }
    if (st.st_size != st_stat.st_size) { printf("size mismatch\n"); return 1; }
    printf("stat cross-check ok\n");

    unlink(FILE_PATH);
    printf("all ok\n");
    return 0;
}
