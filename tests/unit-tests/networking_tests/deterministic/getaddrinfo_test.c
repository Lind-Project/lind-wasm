/*
 * getaddrinfo / freeaddrinfo tests.
 * These are glibc-level functions (not syscalls) that use lower-level
 * socket syscalls internally.
 *
 * Requires /etc/hosts with "127.0.0.1 localhost" and
 * /etc/nsswitch.conf with "hosts: files" in lindfs.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <netdb.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>

int main(void) {
    struct addrinfo hints, *res;
    int ret;

    /* --- 1) Numeric IPv4 address (AI_NUMERICHOST) --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("192.168.1.1", "80", &hints, &res);
    assert(ret == 0);
    assert(res != NULL);

    struct sockaddr_in *sin = (struct sockaddr_in *)res->ai_addr;
    char ipstr[INET_ADDRSTRLEN];
    inet_ntop(AF_INET, &sin->sin_addr, ipstr, sizeof(ipstr));
    assert(strcmp(ipstr, "192.168.1.1") == 0);
    assert(ntohs(sin->sin_port) == 80);
    printf("1. Numeric 192.168.1.1:80 OK\n");
    freeaddrinfo(res);

    /* --- 2) Numeric IPv6 (AI_NUMERICHOST) --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET6;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("::1", "443", &hints, &res);
    assert(ret == 0);
    assert(res->ai_family == AF_INET6);

    struct sockaddr_in6 *sin6 = (struct sockaddr_in6 *)res->ai_addr;
    assert(ntohs(sin6->sin6_port) == 443);

    char ip6str[INET6_ADDRSTRLEN];
    inet_ntop(AF_INET6, &sin6->sin6_addr, ip6str, sizeof(ip6str));
    printf("2. Numeric [%s]:443 OK\n", ip6str);
    freeaddrinfo(res);

    /* --- 3) AI_PASSIVE (for bind/server) --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_PASSIVE;

    ret = getaddrinfo(NULL, "8080", &hints, &res);
    assert(ret == 0);

    sin = (struct sockaddr_in *)res->ai_addr;
    assert(sin->sin_addr.s_addr == htonl(INADDR_ANY));
    assert(ntohs(sin->sin_port) == 8080);
    printf("3. AI_PASSIVE → 0.0.0.0:8080\n");
    freeaddrinfo(res);

    /* --- 4) Error: non-numeric with AI_NUMERICHOST → EAI_NONAME --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("not.a.number", NULL, &hints, &res);
    assert(ret != 0);
    printf("4. AI_NUMERICHOST + non-numeric → error %d (%s)\n",
           ret, gai_strerror(ret));

    /* --- 5) gai_strerror returns non-empty strings --- */
    assert(strlen(gai_strerror(EAI_NONAME)) > 0);
    assert(strlen(gai_strerror(EAI_AGAIN)) > 0);
    assert(strlen(gai_strerror(EAI_MEMORY)) > 0);
    printf("5. gai_strerror OK\n");

    /* --- 6) Resolve "localhost" via /etc/hosts --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    ret = getaddrinfo("localhost", NULL, &hints, &res);
    if (ret == 0) {
        sin = (struct sockaddr_in *)res->ai_addr;
        assert(sin->sin_addr.s_addr == htonl(INADDR_LOOPBACK));
        inet_ntop(AF_INET, &sin->sin_addr, ipstr, sizeof(ipstr));
        printf("6. localhost → %s\n", ipstr);
        freeaddrinfo(res);
    } else {
        /* If /etc/hosts isn't set up, just warn — don't fail */
        printf("6. localhost resolution not available (%s) — skipped\n",
               gai_strerror(ret));
    }

    /* --- 7) Numeric port resolution --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("127.0.0.1", "12345", &hints, &res);
    assert(ret == 0);
    sin = (struct sockaddr_in *)res->ai_addr;
    assert(ntohs(sin->sin_port) == 12345);
    printf("7. Port 12345 resolved OK\n");
    freeaddrinfo(res);

    printf("All getaddrinfo tests passed\n");
    return 0;
}
