/*
 * Test failure scenarios for path conversion and fd handling edge cases.
 *
 * Covers:
 *   - NULL path arguments to path-based syscalls
 *   - PATH_MAX overflow
 *   - Embedded null bytes in path strings
 */

#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <errno.h>
#include <unistd.h>
#include <sys/stat.h>
#include <string.h>
#include <assert.h>
#include <limits.h>  /* PATH_MAX */

int main() {
    int fd;
    int ret;

    /* ---- NULL path tests ---- */

    /* Test 1: open(NULL) - should return -1, not crash */
    errno = 0;
    fd = open(NULL, O_RDONLY);
    assert(fd == -1 && "open(NULL) should return -1");
    printf("Test 1 PASS: open(NULL) returned -1\n");

    /* Test 2: stat(NULL) should return -1, not crash */
    errno = 0;
    struct stat st;
    ret = stat(NULL, &st);
    assert(ret == -1 && "stat(NULL) should return -1");
    printf("Test 2 PASS: stat(NULL) returned -1\n");

    /* Test 3: access(NULL) should return -1, not crash */
    errno = 0;
    ret = access(NULL, F_OK);
    assert(ret == -1 && "access(NULL) should return -1");
    printf("Test 3 PASS: access(NULL) returned -1\n");

    /* Test 4: mkdir(NULL, ...) - should return -1, not crash */
    errno = 0;
    ret = mkdir(NULL, 0755);
    assert(ret == -1 && "mkdir(NULL) should return -1");
    printf("Test 4 PASS: mkdir(NULL) returned -1\n");

    /* Test 5: unlink(NULL) - should return -1, not crash */
    errno = 0;
    ret = unlink(NULL);
    assert(ret == -1 && "unlink(NULL) should return -1");
    printf("Test 5 PASS: unlink(NULL) returned -1\n");

    /* Test 6: link(NULL, NULL) - should return -1, not crash */
    errno = 0;
    ret = link(NULL, NULL);
    assert(ret == -1 && "link(NULL, NULL) should return -1");
    printf("Test 6 PASS: link(NULL, NULL) returned -1\n");

    /* Test 7: rename(NULL, NULL) - should return -1, not crash */
    errno = 0;
    ret = rename(NULL, NULL);
    assert(ret == -1 && "rename(NULL, NULL) should return -1");
    printf("Test 7 PASS: rename(NULL, NULL) returned -1\n");

    /* Test 8: open() with a path exceeding PATH_MAX */
    {
        size_t biglen = PATH_MAX + 256;
        char *bigpath = malloc(biglen + 1);
        assert(bigpath != NULL);
        bigpath[0] = '/';
        memset(bigpath + 1, 'a', biglen - 1);
        bigpath[biglen] = '\0';

        errno = 0;
        fd = open(bigpath, O_RDONLY);
        assert(fd == -1 && "open(huge path) should return -1");
        assert(errno == ENAMETOOLONG && "open(huge path) should set errno to ENAMETOOLONG");
        printf("Test 8 PASS: open(path > PATH_MAX) returned -1 with ENAMETOOLONG\n");
        free(bigpath);
    }

    /* Test 9: open() with embedded null - C truncates at \0,
     * so this becomes open("/nonexistent_path_xyz") which should
     * fail with ENOENT on both native and WASM. */
    {
        char path_with_null[] = "/nonexistent_path_xyz\0/evil";
        errno = 0;
        fd = open(path_with_null, O_RDONLY);
        assert(fd == -1 && "open(path with embedded null) should return -1");
        assert(errno == ENOENT && "open(path with embedded null) should set ENOENT");
        printf("Test 9 PASS: open(path with embedded null) returned -1 with ENOENT\n");
    }
    
    fflush(stdout);
    return 0;
}
