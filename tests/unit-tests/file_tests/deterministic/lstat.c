#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>

const char* TARGET = "testfiles/lstat_target.txt";
const char* SYMLINK = "testfiles/lstat_link";

int main(int argc, char **argv)
{
    // Setup: create target file and symlink
    FILE *fp = fopen(TARGET, "w");
    if (!fp) {
        perror("fopen");
        exit(1);
    }
    fprintf(fp, "hello");
    fclose(fp);

    unlink(SYMLINK);
    if (symlink(TARGET, SYMLINK) < 0) {
        perror("symlink");
        exit(1);
    }

    // Test 1: lstat on regular file should show regular file
    printf("=== Test 1: lstat() on regular file ===\n");
    struct stat st = {0};
    if (lstat(TARGET, &st) < 0) {
        perror("lstat");
        printf("errno: %d\n", errno);
        exit(1);
    }
    if (S_ISREG(st.st_mode)) {
        printf(" lstat() correctly identified regular file\n");
    } else {
        printf(" Error: expected regular file, got mode %o\n", st.st_mode);
        exit(1);
    }

    // Test 2: lstat on symlink should show symlink, NOT the target
    printf("=== Test 2: lstat() on symlink returns symlink mode ===\n");
    struct stat lst = {0};
    if (lstat(SYMLINK, &lst) < 0) {
        perror("lstat");
        printf("errno: %d\n", errno);
        exit(1);
    }
    if (S_ISLNK(lst.st_mode)) {
        printf(" lstat() correctly identified symlink\n");
    } else {
        printf(" Error: expected symlink mode, got %o\n", lst.st_mode);
        exit(1);
    }

    // Test 3: stat on symlink SHOULD follow and show regular file
    printf("=== Test 3: stat() on symlink follows to regular file ===\n");
    struct stat fst = {0};
    if (stat(SYMLINK, &fst) < 0) {
        perror("stat");
        printf("errno: %d\n", errno);
        exit(1);
    }
    if (S_ISREG(fst.st_mode)) {
        printf(" stat() correctly followed symlink to regular file\n");
    } else {
        printf(" Error: expected regular file after follow, got mode %o\n", fst.st_mode);
        exit(1);
    }

    // Test 4: lstat size on symlink should be length of target path string
    printf("=== Test 4: lstat() size on symlink equals target path length ===\n");
    size_t expected_size = strlen(TARGET);
    if ((size_t)lst.st_size == expected_size) {
        printf(" lstat() symlink size correct: %jd\n", lst.st_size);
    } else {
        printf(" Error: expected size %zu, got %jd\n", expected_size, lst.st_size);
        exit(1);
    }

    printf("all lstat tests passed\n");
    return 0;
}