#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/uio.h>

int main(void) {
    const char *path = "testfiles/preadv_pwritev_test.txt";
    int fd = open(path, O_CREAT | O_RDWR | O_TRUNC, 0644);
    assert(fd >= 0);

    /* pwritev: write two buffers at offset 0 */
    char buf1[] = "hello ";
    char buf2[] = "world";
    struct iovec wv[2];
    wv[0].iov_base = buf1;
    wv[0].iov_len = strlen(buf1);
    wv[1].iov_base = buf2;
    wv[1].iov_len = strlen(buf2);

    ssize_t nw = pwritev(fd, wv, 2, 0);
    if (nw < 0) {
        perror("pwritev");
        printf("pwritev returned %zd, errno=%d\n", nw, errno);
    }
    assert(nw == (ssize_t)(strlen(buf1) + strlen(buf2)));

    /* preadv: read back into two buffers at offset 0 */
    char rbuf1[6] = {0};
    char rbuf2[5] = {0};
    struct iovec rv[2];
    rv[0].iov_base = rbuf1;
    rv[0].iov_len = sizeof(rbuf1);
    rv[1].iov_base = rbuf2;
    rv[1].iov_len = sizeof(rbuf2);

    ssize_t nr = preadv(fd, rv, 2, 0);
    assert(nr == 11);
    assert(memcmp(rbuf1, "hello ", 6) == 0);
    assert(memcmp(rbuf2, "world", 5) == 0);

    /* pwritev at non-zero offset */
    char buf3[] = "LIND";
    struct iovec wv2;
    wv2.iov_base = buf3;
    wv2.iov_len = strlen(buf3);

    nw = pwritev(fd, &wv2, 1, 6);
    assert(nw == 4);

    /* verify file pointer unchanged by preadv/pwritev */
    off_t pos = lseek(fd, 0, SEEK_CUR);
    assert(pos == 0);

    /* read full contents to verify */
    char full[12] = {0};
    struct iovec fv;
    fv.iov_base = full;
    fv.iov_len = 11;
    nr = preadv(fd, &fv, 1, 0);
    assert(nr == 11);
    assert(memcmp(full, "hello LINDd", 11) == 0);

    close(fd);
    unlink(path);
    printf("preadv_pwritev: all tests passed\n");
    return 0;
}
