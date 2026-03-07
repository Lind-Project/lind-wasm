/* Test DNS resolution for external hostnames via getaddrinfo.
 *
 * Requires:
 *   - /etc/nsswitch.conf with "hosts: files dns"
 *   - /etc/resolv.conf with valid nameserver entries
 *   - Network access to the configured nameserver
 *
 * Tests:
 *   1. localhost resolves via /etc/hosts (files backend)
 *   2. external hostname resolves via DNS (dns backend)
 *   3. numeric IP resolves without DNS
 */
#include <stdio.h>
#include <string.h>
#include <netdb.h>
#include <arpa/inet.h>
#include <unistd.h>

static int test_resolve(const char *host, int expect_success) {
    struct addrinfo hints, *res;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    int ret = getaddrinfo(host, "80", &hints, &res);
    if (expect_success && ret != 0) {
        char buf[256];
        int len = snprintf(buf, sizeof(buf),
            "FAIL: getaddrinfo(\"%s\") failed: %s\n",
            host, gai_strerror(ret));
        write(2, buf, len);
        return 1;
    }
    if (!expect_success && ret == 0) {
        freeaddrinfo(res);
        char buf[256];
        int len = snprintf(buf, sizeof(buf),
            "FAIL: getaddrinfo(\"%s\") succeeded but expected failure\n", host);
        write(2, buf, len);
        return 1;
    }
    if (expect_success) {
        char addr_str[INET_ADDRSTRLEN];
        struct sockaddr_in *sa = (struct sockaddr_in *)res->ai_addr;
        inet_ntop(AF_INET, &sa->sin_addr, addr_str, sizeof(addr_str));
        char buf[256];
        int len = snprintf(buf, sizeof(buf),
            "OK: %s -> %s\n", host, addr_str);
        write(1, buf, len);
        freeaddrinfo(res);
    }
    return 0;
}

int main(void) {
    int failures = 0;

    /* Test 1: localhost via /etc/hosts */
    failures += test_resolve("localhost", 1);

    /* Test 2: numeric IP (no DNS needed) */
    failures += test_resolve("93.184.216.34", 1);

    /* Test 3: external hostname via DNS */
    failures += test_resolve("example.com", 1);

    if (failures) {
        char buf[64];
        int len = snprintf(buf, sizeof(buf), "%d test(s) failed\n", failures);
        write(2, buf, len);
        return 1;
    }

    write(1, "done\n", 5);
    return 0;
}
