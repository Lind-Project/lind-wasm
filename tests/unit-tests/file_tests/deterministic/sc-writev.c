#include <assert.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/uio.h>
#include <unistd.h>

#define EXPECTED "hello world\n"
#define EXPECTED_LEN (sizeof(EXPECTED) - 1)

int main(void) {
    int sv[2];
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, sv) == 0);

    const char *s0 = "hello";
    const char *s1 = " ";
    const char *s2 = "world";
    const char *s3 = "\n";
    struct iovec iov[4];
    iov[0].iov_base = (void *)s0;
    iov[0].iov_len = strlen(s0);
    iov[1].iov_base = (void *)s1;
    iov[1].iov_len = strlen(s1);
    iov[2].iov_base = (void *)s2;
    iov[2].iov_len = strlen(s2);
    iov[3].iov_base = (void *)s3;
    iov[3].iov_len = strlen(s3);

    ssize_t n = writev(sv[0], iov, 4);
    assert(n == (ssize_t)EXPECTED_LEN);

    char buf[64];
    size_t total = 0;
    while (total < EXPECTED_LEN) {
        ssize_t r = read(sv[1], buf + total, EXPECTED_LEN - total);
        assert(r > 0);
        total += (size_t)r;
    }

    assert(memcmp(buf, EXPECTED, EXPECTED_LEN) == 0);

    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}
