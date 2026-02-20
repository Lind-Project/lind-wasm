/*
 * lat_tcp_selftest.c — Reproduce the exact lmbench lat_tcp server+client flow
 *
 * Mimics the full lat_tcp benchmark in a single program:
 *   fork() → child: server_main (GO_AWAY, SIGCHLD, tcp_server, accept loop with per-connection fork)
 *            parent: sleep, connect, send/recv transaction, measure, shutdown server
 *
 * This is the lmbench lat_tcp pattern minus benchmp.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <signal.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <netinet/in.h>
#include <arpa/inet.h>

#define TCP_PORT 31234

/* lmbench's sigchld handler */
void sigchld_wait_handler(int sig) {
    int status;
    while (waitpid(-1, &status, WNOHANG) > 0)
        ;
}

/* lmbench's doserver — echo protocol */
void doserver(int sock) {
    int n;
    if (read(sock, &n, sizeof(int)) == sizeof(int)) {
        int msize = ntohl(n);
        char *buf = (char *)malloc(msize);
        while (read(sock, buf, msize) > 0) {
            write(sock, buf, msize);
        }
        free(buf);
    } else {
        /* Empty connection = shutdown signal */
        fprintf(stderr, "SERVER: received shutdown\n");
    }
}

/* lmbench's server_main — exact pattern */
void server_main(void) {
    int sock, newsock;
    struct sockaddr_in addr;
    int opt = 1;

    fprintf(stderr, "SERVER: starting (pid=%d)\n", getpid());

    /* GO_AWAY: signal(SIGALRM, exit); alarm(3600); -- skip for now */
    signal(SIGCHLD, sigchld_wait_handler);

    sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock < 0) { perror("SERVER: socket"); _exit(1); }

    setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons(TCP_PORT);

    if (bind(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        perror("SERVER: bind"); _exit(2);
    }
    if (listen(sock, 100) < 0) {
        perror("SERVER: listen"); _exit(3);
    }
    fprintf(stderr, "SERVER: listening on port %d\n", TCP_PORT);

    /* Accept loop with per-connection fork (like lmbench) */
    for (;;) {
        newsock = accept(sock, NULL, NULL);
        if (newsock < 0) {
            if (newsock == -4) continue; /* EINTR */
            perror("SERVER: accept");
            _exit(4);
        }
        fprintf(stderr, "SERVER: accepted connection (fd=%d)\n", newsock);

        switch (fork()) {
        case -1:
            perror("SERVER: fork");
            break;
        case 0:
            close(sock);
            doserver(newsock);
            close(newsock);
            _exit(0);
        default:
            close(newsock);
            break;
        }
    }
}

int main(void) {
    int pid = fork();
    if (pid < 0) { perror("fork"); return 1; }

    if (pid == 0) {
        server_main();
        _exit(0);
    }

    /* ---- PARENT: act as client ---- */
    fprintf(stderr, "CLIENT: waiting for server to start...\n");
    usleep(500000); /* 500ms */

    /* Transaction 1: send msize + data, read echo */
    int sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock < 0) { perror("CLIENT: socket"); return 1; }

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = inet_addr("127.0.0.1");
    addr.sin_port = htons(TCP_PORT);

    fprintf(stderr, "CLIENT: connecting...\n");
    if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        perror("CLIENT: connect");
        kill(pid, SIGTERM);
        waitpid(pid, NULL, 0);
        return 1;
    }
    fprintf(stderr, "CLIENT: connected!\n");

    /* Send msize=1 (like lmbench default) */
    int msize = htonl(1);
    write(sock, &msize, sizeof(int));

    /* Do 10 transactions */
    char buf[1];
    int i;
    buf[0] = 'X';
    for (i = 0; i < 10; i++) {
        write(sock, buf, 1);
        if (read(sock, buf, 1) != 1) {
            fprintf(stderr, "CLIENT: read failed on iteration %d\n", i);
            break;
        }
    }
    close(sock);
    fprintf(stderr, "CLIENT: completed %d transactions\n", i);

    /* Transaction 2: shutdown (empty connect) */
    usleep(100000);
    int shutsock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    connect(shutsock, (struct sockaddr *)&addr, sizeof(addr));
    close(shutsock); /* close immediately = empty read on server = shutdown */

    usleep(100000);
    kill(pid, SIGTERM);
    waitpid(pid, NULL, 0);

    if (i == 10) {
        printf("PASS: lat_tcp server+client pattern works (%d transactions)\n", i);
    } else {
        printf("FAIL: only completed %d/10 transactions\n", i);
    }
    return (i == 10) ? 0 : 1;
}
