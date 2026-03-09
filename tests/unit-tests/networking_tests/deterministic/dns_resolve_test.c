/* Test DNS resolution for localhost and numeric IPs via getaddrinfo.
 *
 * Deterministic: only resolves names that don't require network access.
 *   - localhost via /etc/hosts (files backend)
 *   - numeric IP (no DNS needed)
 *
 * Requires:
 *   - /etc/nsswitch.conf with "hosts: files"
 *   - /etc/hosts with localhost entries
 */
#include <assert.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    struct addrinfo hints, *res;

    /* Test 1: localhost resolves via /etc/hosts */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    int ret = getaddrinfo("localhost", "80", &hints, &res);
    assert(ret == 0);

    struct sockaddr_in *sa = (struct sockaddr_in *)res->ai_addr;
    char addr_str[INET_ADDRSTRLEN];
    inet_ntop(AF_INET, &sa->sin_addr, addr_str, sizeof(addr_str));
    assert(strcmp(addr_str, "127.0.0.1") == 0);
    freeaddrinfo(res);

    /* Test 2: numeric IP resolves without DNS */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("93.184.216.34", "80", &hints, &res);
    assert(ret == 0);

    sa = (struct sockaddr_in *)res->ai_addr;
    inet_ntop(AF_INET, &sa->sin_addr, addr_str, sizeof(addr_str));
    assert(strcmp(addr_str, "93.184.216.34") == 0);
    freeaddrinfo(res);

    printf("dns_resolve_test ok\n");
    return 0;
}
