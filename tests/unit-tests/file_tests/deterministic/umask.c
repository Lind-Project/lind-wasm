/*
 * Before running this test:
 *   1. Ensure the test directory exists in $LIND_FS_ROOT.
 *   2. No pre-existing files named "umask_test_file.txt" or "umask_test_dir".
 */


#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <stdio.h>

int main() {
    mode_t old_mask;
    int fd;
    struct stat st;

     /* Cleanup from any previous run */
    rmdir("testfiles/umask_test_dir");
    unlink("testfiles/umask_test_file.txt");
    unlink("testfiles/umask_test_file2.txt");


    /* Test 1: umask returns previous mask */
    old_mask = umask(0022);
    if (old_mask != 0022) {
        printf("FAIL: expected initial umask 0022, got %04o\n", old_mask);
        return 1;
    }
    umask(old_mask); /* restore */
    printf("PASS: umask returns previous mask\n");

    /* Test 2: umask applied on open — file created with 0666 & ~0022 = 0644 */
    umask(0022);
    fd = open("testfiles/umask_test_file.txt", O_CREAT | O_WRONLY, 0666);
    if (fd == -1) {
        perror("open failed");
        return 1;
    }
    close(fd);

    if (stat("testfiles/umask_test_file.txt", &st) == -1) {
        perror("stat failed");
        return 1;
    }
    if ((st.st_mode & 0777) != 0644) {
        printf("FAIL: expected file perms 0644, got %04o\n", st.st_mode & 0777);
        return 1;
    }
    printf("PASS: umask applied correctly on open (0644)\n");

    /* Test 3: umask applied on mkdir — dir created with 0777 & ~0022 = 0755 */
    umask(0022);
    if (mkdir("testfiles/umask_test_dir", 0777) == -1) {
        perror("mkdir failed");
        return 1;
    }
    if (stat("testfiles/umask_test_dir", &st) == -1) {
        perror("stat on dir failed");
        return 1;
    }
    if ((st.st_mode & 0777) != 0755) {
        printf("FAIL: expected dir perms 0755, got %04o\n", st.st_mode & 0777);
        return 1;
    }
    printf("PASS: umask applied correctly on mkdir (0755)\n");

    /* Test 4: changing umask affects subsequent creates */
    umask(0077);
    fd = open("testfiles/umask_test_file2.txt", O_CREAT | O_WRONLY, 0666);
    if (fd == -1) {
        perror("open failed");
        return 1;
    }
    close(fd);
    if (stat("testfiles/umask_test_file2.txt", &st) == -1) {
        perror("stat failed");
        return 1;
    }
    if ((st.st_mode & 0777) != 0600) {
        printf("FAIL: expected file perms 0600, got %04o\n", st.st_mode & 0777);
        return 1;
    }
    printf("PASS: umask 0077 applied correctly on open (0600)\n");

    printf("All umask tests passed.\n");
    return 0;
}
