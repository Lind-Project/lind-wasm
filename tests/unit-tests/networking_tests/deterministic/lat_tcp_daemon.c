/*
 * lat_tcp_daemon.c — Test the POSIX daemon pattern with our fix
 *
 * Mimics the real lat_tcp flow:
 *   1. fork() → child becomes server, parent exits (daemon pattern)
 *   2. grandparent (main) waits, then forks a client
 *   3. client connects to server, does transactions, shuts down server
 *
 * This tests whether child cages survive parent exit (the fix).
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

void sigchld_handler(int sig) {
    int status;
    while (waitpid(-1, &status, WNOHANG) > 0)
        ;
}

/* Server: accept one connection, echo, then exit */
void server_main(void) {
    int sock, newsock;
    struct sockaddr_in addr;
    int opt = 1;
    int n;

    signal(SIGCHLD, sigchld_handler);

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
    fprintf(stderr, "SERVER: listening on port %d (pid=%d)\n", TCP_PORT, getpid());

    /* Accept two connections: one data, one shutdown */
    int conn_count = 0;
    while (conn_count < 2) {
        newsock = accept(sock, NULL, NULL);
        if (newsock < 0) continue;
        conn_count++;
        fprintf(stderr, "SERVER: accepted connection #%d\n", conn_count);

        /* Read msize header */
        if (read(newsock, &n, sizeof(int)) == sizeof(int)) {
            int msize = ntohl(n);
            char *buf = (char *)malloc(msize);
            while (read(newsock, buf, msize) > 0) {
                write(newsock, buf, msize);
            }
            free(buf);
            fprintf(stderr, "SERVER: finished serving connection #%d\n", conn_count);
        } else {
            fprintf(stderr, "SERVER: shutdown received\n");
        }
        close(newsock);
    }

    close(sock);
    fprintf(stderr, "SERVER: exiting\n");
    _exit(0);
}

int main(void) {
    int server_launcher;

    /* Step 1: Fork the "launcher" (like lat_tcp -s) */
    server_launcher = fork();
    if (server_launcher < 0) { perror("fork1"); return 1; }

    if (server_launcher == 0) {
        /* Launcher: fork server child, then EXIT (daemon pattern) */
        int server_child = fork();
        if (server_child < 0) { perror("fork2"); _exit(1); }
        if (server_child == 0) {
            server_main();  /* grandchild becomes server */
        }
        fprintf(stderr, "LAUNCHER: forked server (pid=%d), exiting\n", server_child);
        _exit(0);  /* parent exits — this is the daemon pattern */
    }

    /* Main process: wait for launcher to exit */
    waitpid(server_launcher, NULL, 0);
    fprintf(stderr, "MAIN: launcher exited, waiting for server to be ready...\n");
    usleep(500000);  /* 500ms for server to bind+listen */

    /* Step 2: Fork a client */
    int client = fork();
    if (client < 0) { perror("fork3"); return 1; }

    if (client == 0) {
        /* Client process */
        int sock;
        struct sockaddr_in addr;

        sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (sock < 0) { perror("CLIENT: socket"); _exit(1); }

        memset(&addr, 0, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = inet_addr("127.0.0.1");
        addr.sin_port = htons(TCP_PORT);

        fprintf(stderr, "CLIENT: connecting...\n");
        if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
            perror("CLIENT: connect");
            _exit(1);
        }
        fprintf(stderr, "CLIENT: connected!\n");

        /* Send msize=1, do 10 transactions */
        int msize = htonl(1);
        write(sock, &msize, sizeof(int));

        char buf[1];
        buf[0] = 'X';
        int i;
        for (i = 0; i < 10; i++) {
            write(sock, buf, 1);
            if (read(sock, buf, 1) != 1) {
                fprintf(stderr, "CLIENT: read failed at iteration %d\n", i);
                break;
            }
        }
        close(sock);
        fprintf(stderr, "CLIENT: completed %d transactions\n", i);

        /* Send shutdown (empty connection) */
        usleep(100000);
        int shutsock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (shutsock >= 0) {
            connect(shutsock, (struct sockaddr *)&addr, sizeof(addr));
            close(shutsock);
        }

        _exit(i == 10 ? 0 : 1);
    }

    /* Main: wait for client */
    int status;
    waitpid(client, &status, 0);

    if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
        printf("PASS: daemon pattern server+client works\n");
    } else {
        printf("FAIL: client exited with status %d\n", WEXITSTATUS(status));
    }

    return WIFEXITED(status) && WEXITSTATUS(status) == 0 ? 0 : 1;
}
