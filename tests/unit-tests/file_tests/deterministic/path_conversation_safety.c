/*
Write failure scenarios for path conversion exploits
*/

#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <errno.h>
#include <unistd.h>
#include <sys/stat.h>
#include <string.h>

int main() {
    int fd;
    int ret;

    /*Test 1: open(NULL) - should return -1, not crash */
    errno = 0;
    fd = open(NULL, O_RDONLY);
    if (fd==-1) {
        printf("Test 1 Pass: open(NULL) return -1, errno=%d\n", errno);
    } else {
        printf("Test 1 Fail: open(NULL) returned fd=%d", fd);
        close(fd);
    }

    /*Test 2: stat(NULL) should return -1, not crash*/
    errno = 0;
    struct stat st;
    ret = stat(NULL, &st);
    if (ret == -1) {
        printf("Test 2: Pass stat(NULL) return -1, errno=%d\n", errno);
    } else {
        printf("Test 2: Fail stat(NULL) returned %d\n", ret);

    }

    /*Test 3: access(NULL) should return -1, not crash */
    errno = 0;
    ret = access(NULL, F_OK);
    if (ret==-1) {
        printf("Test 3: Pass access(NULL) returns -1, errno=%d\n", errno);
    } else {
        printf("Test 3: Fail access(NULL) returned %d\n", ret);
    }

    /* Test 4: open("", ...) — empty string, should return -1 with ENOENT */
    errno = 0;
    fd = open("", O_RDONLY);
    if (fd == -1) {
        printf("Test 4 PASS: open(\"\") returned -1, errno=%d\n", errno);
    } else {
        printf("Test 4 FAIL: open(\"\") returned fd=%d\n", fd);
        close(fd);
    }

    /* Test 5: mkdir(NULL, ...) — should return -1, not crash */
    errno = 0;
    ret = mkdir(NULL, 0755);
    if (ret == -1) {
        printf("Test 5 PASS: mkdir(NULL) returned -1, errno=%d\n", errno);
    } else {
        printf("Test 5 FAIL: mkdir(NULL) returned %d \n", ret);
    }

    /* Test 6: unlink(NULL) — should return -1, not crash */
    errno = 0;
    ret = unlink(NULL);
    if (ret == -1) {
        printf("Test 6 PASS: unlink(NULL) returned -1, errno=%d\n", errno);
    } else {
        printf("Test 6 FAIL: unlink(NULL) returned %d\n", ret);
    }

    /* Test 7: link(NULL, NULL) — should return -1, not crash */
    errno = 0;
    ret = link(NULL, NULL);
    if (ret == -1) {
        printf("Test 7 PASS: link(NULL, NULL) returned -1, errno=%d\n", errno);
    } else {
        printf("Test 7 FAIL: link(NULL, NULL) returned %d)\n", ret);
    }

    /* Test 8: rename(NULL, NULL) — should return -1, not crash */
    errno = 0;
    ret = rename(NULL, NULL);
    if (ret == -1) {
        printf("Test 8 PASS: rename(NULL, NULL) returned -1, errno=%d\n", errno);
    } else {
        printf("Test 8 FAIL: rename(NULL, NULL) returned %d\n", ret);
    }

    printf("All path_conversion safety tests completed without crash.\n");
    fflush(stdout);
    return 0;
}