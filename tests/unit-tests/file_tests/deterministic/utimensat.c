/* Tests for utimensat / futimens / stat time-field plumbing.
 *
 * Exercises:
 *   - utimensat with AT_FDCWD + a path: the explicit (atime, mtime) we
 *     pass must be readable via stat() afterwards.
 *   - futimens (which glibc implements as utimensat(fd, NULL, ts, 0)):
 *     same round-trip via fstat.
 *   - UTIME_NOW / UTIME_OMIT: setting atime to NOW while OMITting mtime
 *     must change atime but leave mtime untouched.
 *
 * Failure modes this catches:
 *   - utimensat unimplemented (silently no-ops, leaving mtime at 0).
 *   - convert_statdata_to_user not copying st_atim / st_mtim / st_ctim
 *     (so even a working utimensat would round-trip as 0).
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define TEST_PATH "testfiles/utimensat-target"

static void create_file(const char *path)
{
    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        perror("open(create)");
        exit(1);
    }
    close(fd);
}

static void check_times(const struct stat *st, time_t want_atime,
                        time_t want_mtime, const char *who)
{
    if (st->st_atime != want_atime) {
        fprintf(stderr,
                "FAIL [%s]: st_atime = %lld, expected %lld\n",
                who, (long long)st->st_atime, (long long)want_atime);
        exit(1);
    }
    if (st->st_mtime != want_mtime) {
        fprintf(stderr,
                "FAIL [%s]: st_mtime = %lld, expected %lld\n",
                who, (long long)st->st_mtime, (long long)want_mtime);
        exit(1);
    }
}

int main(void)
{
    create_file(TEST_PATH);

    /* ---- 1. utimensat(AT_FDCWD, path, [a, m], 0) round-trip ---- */
    struct timespec ts1[2];
    ts1[0].tv_sec = 1234567890;  /* atime: 2009-02-13 */
    ts1[0].tv_nsec = 0;
    ts1[1].tv_sec = 1000000000;  /* mtime: 2001-09-09 */
    ts1[1].tv_nsec = 0;
    if (utimensat(AT_FDCWD, TEST_PATH, ts1, 0) != 0) {
        perror("utimensat(AT_FDCWD)");
        exit(1);
    }
    struct stat st1;
    if (stat(TEST_PATH, &st1) != 0) {
        perror("stat after utimensat");
        exit(1);
    }
    check_times(&st1, ts1[0].tv_sec, ts1[1].tv_sec, "utimensat round-trip");

    /* ---- 2. futimens via an open fd ---- */
    int fd = open(TEST_PATH, O_RDONLY);
    if (fd < 0) {
        perror("open for futimens");
        exit(1);
    }
    struct timespec ts2[2];
    ts2[0].tv_sec = 1500000000;  /* atime: 2017-07-14 */
    ts2[0].tv_nsec = 0;
    ts2[1].tv_sec = 1600000000;  /* mtime: 2020-09-13 */
    ts2[1].tv_nsec = 0;
    if (futimens(fd, ts2) != 0) {
        perror("futimens");
        close(fd);
        exit(1);
    }
    struct stat st2;
    if (fstat(fd, &st2) != 0) {
        perror("fstat after futimens");
        close(fd);
        exit(1);
    }
    check_times(&st2, ts2[0].tv_sec, ts2[1].tv_sec, "futimens round-trip");
    close(fd);

    /* ---- 3. UTIME_OMIT preserves the omitted field ---- */
    /* Save current mtime, then bump only atime via UTIME_NOW + OMIT. */
    struct stat st_before;
    if (stat(TEST_PATH, &st_before) != 0) {
        perror("stat before UTIME_OMIT");
        exit(1);
    }
    time_t saved_mtime = st_before.st_mtime;

    struct timespec ts3[2];
    ts3[0].tv_sec  = 0; ts3[0].tv_nsec = UTIME_NOW;
    ts3[1].tv_sec  = 0; ts3[1].tv_nsec = UTIME_OMIT;
    if (utimensat(AT_FDCWD, TEST_PATH, ts3, 0) != 0) {
        perror("utimensat(UTIME_NOW/OMIT)");
        exit(1);
    }
    struct stat st_after;
    if (stat(TEST_PATH, &st_after) != 0) {
        perror("stat after UTIME_OMIT");
        exit(1);
    }
    if (st_after.st_mtime != saved_mtime) {
        fprintf(stderr,
                "FAIL [UTIME_OMIT]: st_mtime changed from %lld to %lld\n",
                (long long)saved_mtime, (long long)st_after.st_mtime);
        exit(1);
    }
    if (st_after.st_atime == st_before.st_atime
        || st_after.st_atime <= ts2[0].tv_sec) {
        /* atime should now reflect "now", which is well after our 2017 mark. */
        fprintf(stderr,
                "FAIL [UTIME_NOW]: st_atime did not advance "
                "(before=%lld, after=%lld)\n",
                (long long)st_before.st_atime,
                (long long)st_after.st_atime);
        exit(1);
    }

    /* ---- cleanup ---- */
    unlink(TEST_PATH);
    printf("utimensat: PASS\n");
    return 0;
}
