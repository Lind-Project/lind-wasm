/*
 * tcp_fork_server.c — Isolate the lmbench lat_tcp server pattern
 *
 * Mimics lat_tcp's server_main():
 *   fork() → child: socket/bind/listen/accept → echo
 *            parent: connect to child, send/recv, verify
 *
 * This tests whether a forked child can successfully run a TCP server.
 * Progressive checkpoints print where we get to.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <errno.h>

#define TEST_PORT 31234  /* Same port as lmbench TCP_XACT */
#define MSG "hello from parent"

int main(void) {
    int pid;
    int status;

    pid = fork();
    if (pid < 0) {
        perror("fork");
        return 1;
    }

    if (pid == 0) {
        /* ---- CHILD: TCP server (mimics lmbench server_main) ---- */
        int sock, newsock;
        struct sockaddr_in addr;
        int opt = 1;
        char buf[256];
        ssize_t n;

        fprintf(stderr, "CHILD: started (pid=%d)\n", getpid());

        sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (sock < 0) {
            perror("CHILD: socket");
            _exit(1);
        }
        fprintf(stderr, "CHILD: socket ok (fd=%d)\n", sock);

        setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

        memset(&addr, 0, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = INADDR_ANY;
        addr.sin_port = htons(TEST_PORT);

        if (bind(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
            perror("CHILD: bind");
            _exit(2);
        }
        fprintf(stderr, "CHILD: bind ok (port %d)\n", TEST_PORT);

        if (listen(sock, 100) < 0) {
            perror("CHILD: listen");
            _exit(3);
        }
        fprintf(stderr, "CHILD: listen ok, waiting for accept...\n");

        newsock = accept(sock, NULL, NULL);
        if (newsock < 0) {
            perror("CHILD: accept");
            _exit(4);
        }
        fprintf(stderr, "CHILD: accept ok (fd=%d)\n", newsock);

        /* Echo one message back */
        n = read(newsock, buf, sizeof(buf) - 1);
        if (n > 0) {
            buf[n] = '\0';
            fprintf(stderr, "CHILD: received \"%s\"\n", buf);
            write(newsock, buf, n);
        }

        close(newsock);
        close(sock);
        fprintf(stderr, "CHILD: done\n");
        _exit(0);

    } else {
        /* ---- PARENT: TCP client ---- */
        int sock;
        struct sockaddr_in addr;
        char buf[256];
        ssize_t n;

        fprintf(stderr, "PARENT: forked child pid=%d\n", pid);

        /* Give child time to bind+listen */
        usleep(500000);  /* 500ms */

        sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (sock < 0) {
            perror("PARENT: socket");
            return 1;
        }
        fprintf(stderr, "PARENT: socket ok (fd=%d)\n", sock);

        memset(&addr, 0, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = inet_addr("127.0.0.1");
        addr.sin_port = htons(TEST_PORT);

        fprintf(stderr, "PARENT: connecting to 127.0.0.1:%d...\n", TEST_PORT);
        if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
            perror("PARENT: connect");
            /* If connect fails, child is probably not listening */
            waitpid(pid, &status, WNOHANG);
            return 1;
        }
        fprintf(stderr, "PARENT: connected!\n");

        /* Send message */
        write(sock, MSG, strlen(MSG));
        fprintf(stderr, "PARENT: sent \"%s\"\n", MSG);

        /* Read echo */
        n = read(sock, buf, sizeof(buf) - 1);
        if (n > 0) {
            buf[n] = '\0';
            fprintf(stderr, "PARENT: received \"%s\"\n", buf);
            if (strcmp(buf, MSG) == 0) {
                printf("PASS: fork+tcp server/client echo works\n");
            } else {
                printf("FAIL: echo mismatch: sent \"%s\", got \"%s\"\n", MSG, buf);
            }
        } else {
            printf("FAIL: no data received from child server\n");
        }

        close(sock);
        waitpid(pid, &status, 0);
        fprintf(stderr, "PARENT: child exited with status %d\n", WEXITSTATUS(status));
    }

    return 0;
}
