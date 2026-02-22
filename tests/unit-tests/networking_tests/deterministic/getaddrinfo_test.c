/*
 * getaddrinfo / getnameinfo / freeaddrinfo tests.
 * These are glibc-level functions (not syscalls) that use lower-level
 * socket syscalls internally.
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
    struct addrinfo hints, *res, *p;
    int ret;

    /* --- 1) Resolve "localhost" → should get 127.0.0.1 or ::1 --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET; /* force IPv4 */
    hints.ai_socktype = SOCK_STREAM;

    ret = getaddrinfo("localhost", NULL, &hints, &res);
    assert(ret == 0);
    assert(res != NULL);
    assert(res->ai_family == AF_INET);

    struct sockaddr_in *sin = (struct sockaddr_in *)res->ai_addr;
    assert(sin->sin_addr.s_addr == htonl(INADDR_LOOPBACK));

    char ipstr[INET_ADDRSTRLEN];
    inet_ntop(AF_INET, &sin->sin_addr, ipstr, sizeof(ipstr));
    printf("1. localhost → %s\n", ipstr);

    freeaddrinfo(res);

    /* --- 2) Numeric address (AI_NUMERICHOST) --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("192.168.1.1", "80", &hints, &res);
    assert(ret == 0);
    assert(res != NULL);

    sin = (struct sockaddr_in *)res->ai_addr;
    inet_ntop(AF_INET, &sin->sin_addr, ipstr, sizeof(ipstr));
    assert(strcmp(ipstr, "192.168.1.1") == 0);
    assert(ntohs(sin->sin_port) == 80);
    printf("2. Numeric 192.168.1.1:80 → %s:%d\n", ipstr, ntohs(sin->sin_port));

    freeaddrinfo(res);

    /* --- 3) Service name resolution (port) --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("127.0.0.1", "80", &hints, &res);
    assert(ret == 0);

    sin = (struct sockaddr_in *)res->ai_addr;
    assert(ntohs(sin->sin_port) == 80);
    printf("3. Port '80' resolved to %d\n", ntohs(sin->sin_port));

    freeaddrinfo(res);

    /* --- 4) AI_PASSIVE (for bind/server) --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_PASSIVE;

    ret = getaddrinfo(NULL, "8080", &hints, &res);
    assert(ret == 0);

    sin = (struct sockaddr_in *)res->ai_addr;
    assert(sin->sin_addr.s_addr == htonl(INADDR_ANY));
    assert(ntohs(sin->sin_port) == 8080);
    printf("4. AI_PASSIVE → 0.0.0.0:8080\n");

    freeaddrinfo(res);

    /* --- 5) Error: invalid hostname with AI_NUMERICHOST --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_flags = AI_NUMERICHOST;

    ret = getaddrinfo("not.a.number", NULL, &hints, &res);
    assert(ret != 0); /* EAI_NONAME */
    printf("5. AI_NUMERICHOST + non-numeric → error %d (%s)\n",
           ret, gai_strerror(ret));

    /* --- 6) gai_strerror for various codes --- */
    assert(strlen(gai_strerror(EAI_NONAME)) > 0);
    assert(strlen(gai_strerror(EAI_AGAIN)) > 0);
    assert(strlen(gai_strerror(EAI_MEMORY)) > 0);
    printf("6. gai_strerror returns non-empty strings\n");

    /* --- 7) IPv6 numeric --- */
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
    printf("7. IPv6 ::1:443 → [%s]:%d\n", ip6str, ntohs(sin6->sin6_port));

    freeaddrinfo(res);

    /* --- 8) Multiple results with AF_UNSPEC --- */
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    ret = getaddrinfo("localhost", "80", &hints, &res);
    assert(ret == 0);

    int count = 0;
    int has_v4 = 0, has_v6 = 0;
    for (p = res; p != NULL; p = p->ai_next) {
        count++;
        if (p->ai_family == AF_INET) has_v4 = 1;
        if (p->ai_family == AF_INET6) has_v6 = 1;
    }
    assert(count >= 1);
    printf("8. AF_UNSPEC localhost → %d result(s), v4=%d v6=%d\n",
           count, has_v4, has_v6);

    freeaddrinfo(res);

    printf("All getaddrinfo tests passed\n");
    return 0;
}
