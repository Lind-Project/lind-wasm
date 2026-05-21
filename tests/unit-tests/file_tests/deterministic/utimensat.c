/* Tests for utimensat / futimens / utimes / stat time-field plumbing.
 *
 * Exercises:
 *   - utimensat with AT_FDCWD + a path: explicit (atime, mtime) round-trips
 *     through stat.
 *   - futimens (glibc routes via utimensat(fd, NULL, ts, 0)): same round
 *     trip via fstat.
 *   - UTIME_NOW / UTIME_OMIT semantics (atime advances, mtime preserved).
 *   - utimes (legacy μs API) funnels through the same __utimensat64_helper
 *     in lind-wasm glibc — exercising one legacy variant covers the funnel.
 *
 * Catches utimensat being unimplemented (round-trips read 0) and
 * convert_statdata_to_user not copying st_atim / st_mtim / st_ctim.
 */

#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>
#include <utime.h>

#define TEST_PATH "testfiles/utimensat-target"

int main(void)
{
    int fd = open(TEST_PATH, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    close(fd);

    /* 1. utimensat(AT_FDCWD, path, [a, m], 0) round-trip. */
    struct timespec ts1[2];
    ts1[0].tv_sec = 1234567890; ts1[0].tv_nsec = 0;  /* atime */
    ts1[1].tv_sec = 1000000000; ts1[1].tv_nsec = 0;  /* mtime */
    assert(utimensat(AT_FDCWD, TEST_PATH, ts1, 0) == 0);

    struct stat st1;
    assert(stat(TEST_PATH, &st1) == 0);
    assert(st1.st_atime == ts1[0].tv_sec);
    assert(st1.st_mtime == ts1[1].tv_sec);

    /* 2. futimens via an open fd. */
    fd = open(TEST_PATH, O_RDONLY);
    assert(fd >= 0);

    struct timespec ts2[2];
    ts2[0].tv_sec = 1500000000; ts2[0].tv_nsec = 0;
    ts2[1].tv_sec = 1600000000; ts2[1].tv_nsec = 0;
    assert(futimens(fd, ts2) == 0);

    struct stat st2;
    assert(fstat(fd, &st2) == 0);
    assert(st2.st_atime == ts2[0].tv_sec);
    assert(st2.st_mtime == ts2[1].tv_sec);
    close(fd);

    /* 3. UTIME_OMIT preserves the omitted field; UTIME_NOW advances. */
    struct stat st_before;
    assert(stat(TEST_PATH, &st_before) == 0);
    time_t saved_mtime = st_before.st_mtime;

    struct timespec ts3[2];
    ts3[0].tv_sec = 0; ts3[0].tv_nsec = UTIME_NOW;
    ts3[1].tv_sec = 0; ts3[1].tv_nsec = UTIME_OMIT;
    assert(utimensat(AT_FDCWD, TEST_PATH, ts3, 0) == 0);

    struct stat st_after;
    assert(stat(TEST_PATH, &st_after) == 0);
    assert(st_after.st_mtime == saved_mtime);
    /* atime should be "now", well after our 2017 mark from step 2. */
    assert(st_after.st_atime > ts2[0].tv_sec);

    /* 4. utimes (legacy API) funnels through __utimensat64_helper. */
    struct timeval tv[2];
    tv[0].tv_sec = 1700000000; tv[0].tv_usec = 0;
    tv[1].tv_sec = 1750000000; tv[1].tv_usec = 0;
    assert(utimes(TEST_PATH, tv) == 0);

    struct stat st4;
    assert(stat(TEST_PATH, &st4) == 0);
    assert(st4.st_atime == tv[0].tv_sec);
    assert(st4.st_mtime == tv[1].tv_sec);

    unlink(TEST_PATH);
    return 0;
}
