/*
 * Test: getaddrinfo with PF_UNSPEC resolves both IPv4 and IPv6.
 *
 * Most applications (curl, wget, etc.) use PF_UNSPEC by default.
 * This exercises the dual A+AAAA DNS query path in glibc's resolver,
 * which requires sendmmsg fallback to individual send() calls.
 */

#include <assert.h>
#include <netdb.h>
#include <string.h>

int main(void)
{
    struct addrinfo hints, *res;

    /* AF_INET should work */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    int ret = getaddrinfo("example.com", "80", &hints, &res);
    assert(ret == 0);
    freeaddrinfo(res);

    /* PF_UNSPEC should also work */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = PF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    ret = getaddrinfo("example.com", "80", &hints, &res);
    assert(ret == 0);
    freeaddrinfo(res);

    return 0;
}
