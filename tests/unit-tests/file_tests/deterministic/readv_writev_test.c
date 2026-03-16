#include <unistd.h>
#include <string.h>
#include <fcntl.h>
#include <sys/uio.h>
#include <sys/stat.h>
#include <stdio.h>
#include <stdlib.h>

int main() {
    const char *filename = "testfiles/readv_writev_test.txt";
    mkdir("testfiles", 0755);

    int fd = open(filename, O_RDWR | O_CREAT | O_TRUNC, 0777);
    if (fd == -1) {
        perror("open");
        return 1;
    }

    /* writev: scatter 3 buffers into the file */
    char *w1 = "alpha-";
    char *w2 = "bravo-";
    char *w3 = "charlie";
    struct iovec wv[3];
    wv[0].iov_base = w1; wv[0].iov_len = strlen(w1);
    wv[1].iov_base = w2; wv[1].iov_len = strlen(w2);
    wv[2].iov_base = w3; wv[2].iov_len = strlen(w3);

    ssize_t nw = writev(fd, wv, 3);
    if (nw == -1) {
        perror("writev");
        close(fd);
        return 1;
    }

    const char *expected = "alpha-bravo-charlie";
    size_t total = strlen(expected);
    if ((size_t)nw != total) {
        printf("writev: expected %zu bytes, got %zd\n", total, nw);
        close(fd);
        return 1;
    }

    /* lseek back to start */
    if (lseek(fd, 0, SEEK_SET) == -1) {
        perror("lseek");
        close(fd);
        return 1;
    }

    /* readv: gather into 3 separate buffers matching the write segments */
    char r1[7] = {0};
    char r2[7] = {0};
    char r3[8] = {0};
    struct iovec rv[3];
    rv[0].iov_base = r1; rv[0].iov_len = 6;  /* "alpha-" */
    rv[1].iov_base = r2; rv[1].iov_len = 6;  /* "bravo-" */
    rv[2].iov_base = r3; rv[2].iov_len = 7;  /* "charlie" */

    ssize_t nr = readv(fd, rv, 3);
    if (nr == -1) {
        perror("readv");
        close(fd);
        return 1;
    }
    if ((size_t)nr != total) {
        printf("readv: expected %zu bytes, got %zd\n", total, nr);
        close(fd);
        return 1;
    }

    if (strcmp(r1, "alpha-") != 0 ||
        strcmp(r2, "bravo-") != 0 ||
        strcmp(r3, "charlie") != 0) {
        printf("readv content mismatch: [%s][%s][%s]\n", r1, r2, r3);
        close(fd);
        return 1;
    }

    close(fd);
    printf("readv_writev_test passed\n");
    return 0;
}
