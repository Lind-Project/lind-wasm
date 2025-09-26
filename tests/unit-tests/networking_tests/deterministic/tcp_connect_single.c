#define _GNU_SOURCE
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <string.h>
#include <errno.h>
#include <stdlib.h>

int main(void) {
    // ---- 1) server: socket + setsockopt + bind(127.0.0.1:49153) + listen ----
    int s_listen = socket(AF_INET, SOCK_STREAM, 0);
    if (s_listen < 0) return 1;

    int yes = 1;
    if (setsockopt(s_listen, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes)) < 0)
        return 2;

    struct sockaddr_in srv = {0};
    srv.sin_family = AF_INET;
    srv.sin_addr.s_addr = htonl(INADDR_LOOPBACK);  // 127.0.0.1
    srv.sin_port = htons(49153);                 

    if (bind(s_listen, (struct sockaddr *)&srv, sizeof(srv)) < 0)
        return 3;

    if (listen(s_listen, 1) < 0)
        return 4;

    // ---- 2) client: socket + connect(127.0.0.1:49153) ----
    int s_client = socket(AF_INET, SOCK_STREAM, 0);
    if (s_client < 0)
        return 5;

    struct sockaddr_in dst = srv; // 同 127.0.0.1:49153
    if (connect(s_client, (struct sockaddr *)&dst, sizeof(dst)) < 0)
        return 6;

    // ---- 3) server: accept ----
    int s_conn = accept(s_listen, NULL, NULL);
    if (s_conn < 0)
        return 7;

    // ---- 4) send ----
    static const char msg1[] = "ping";
    if (send(s_client, msg1, sizeof(msg1)-1, 0) < 0)
        return 8;

    static const char msg2[] = "pong";
    if (send(s_conn, msg2, sizeof(msg2)-1, 0) < 0)
        return 9;

    
    return 0;
}
