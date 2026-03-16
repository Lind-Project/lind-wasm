/*
 * Scatter-gather I/O: writev() on sockets and pipes.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <sys/uio.h>
#include <sys/socket.h>

int main(void) {
    int pair[2];

    /* --- 1) writev on socketpair --- */
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);

    char buf1[] = "Hello";
    char buf2[] = ", ";
    char buf3[] = "World!";

    struct iovec iov[3];
    iov[0].iov_base = buf1;
    iov[0].iov_len = strlen(buf1);
    iov[1].iov_base = buf2;
    iov[1].iov_len = strlen(buf2);
    iov[2].iov_base = buf3;
    iov[2].iov_len = strlen(buf3);

    ssize_t total_expected = strlen(buf1) + strlen(buf2) + strlen(buf3);
    ssize_t nw = writev(pair[0], iov, 3);
    assert(nw == total_expected);

    char result[64] = {0};
    ssize_t nr = read(pair[1], result, sizeof(result));
    assert(nr == total_expected);
    assert(strcmp(result, "Hello, World!") == 0);
    printf("1. writev on socketpair: \"%s\" (%zd bytes)\n", result, nr);

    close(pair[0]);
    close(pair[1]);

    /* --- 2) writev on pipe --- */
    int p[2];
    assert(pipe(p) == 0);

    char a[] = "foo";
    char b[] = "bar";
    iov[0].iov_base = a;
    iov[0].iov_len = 3;
    iov[1].iov_base = b;
    iov[1].iov_len = 3;

    nw = writev(p[1], iov, 2);
    assert(nw == 6);

    memset(result, 0, sizeof(result));
    nr = read(p[0], result, sizeof(result));
    assert(nr == 6);
    assert(memcmp(result, "foobar", 6) == 0);
    printf("2. writev on pipe: \"%.*s\"\n", (int)nr, result);

    close(p[0]);
    close(p[1]);

    /* --- 3) writev with zero-length iovec entry --- */
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);

    char c[] = "start";
    char d[] = "end";
    iov[0].iov_base = c;
    iov[0].iov_len = 5;
    iov[1].iov_base = NULL;
    iov[1].iov_len = 0; /* zero-length in the middle */
    iov[2].iov_base = d;
    iov[2].iov_len = 3;

    nw = writev(pair[0], iov, 3);
    assert(nw == 8);

    memset(result, 0, sizeof(result));
    nr = read(pair[1], result, sizeof(result));
    assert(nr == 8);
    assert(memcmp(result, "startend", 8) == 0);
    printf("3. writev with zero-length iov: \"%.*s\"\n", (int)nr, result);

    close(pair[0]);
    close(pair[1]);

    /* --- 4) Single iovec (degenerate case) --- */
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);

    char single[] = "only";
    iov[0].iov_base = single;
    iov[0].iov_len = 4;

    nw = writev(pair[0], iov, 1);
    assert(nw == 4);

    memset(result, 0, sizeof(result));
    nr = read(pair[1], result, sizeof(result));
    assert(nr == 4);
    assert(memcmp(result, "only", 4) == 0);
    printf("4. writev single iovec: \"%.*s\"\n", (int)nr, result);

    close(pair[0]);
    close(pair[1]);

    printf("All writev tests passed\n");
    return 0;
}
