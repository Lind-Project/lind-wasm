/*
 * AF_UNIX bidirectional roundtrip test.
 *
 * Server: bind/listen/accept, write a tag, read a tag back, verify.
 * Client: connect, read server's tag, verify, write its own tag.
 *
 * Failure modes this catches:
 *   - send/recv pipes are paired in the wrong direction at accept-time.
 *   - read/write handlers route the wrong pipe for an IPC_SOCKET.
 *   - data written by either side is silently dropped.
 *
 * Passes natively and should pass under any correct UDS implementation.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

#define SOCK_PATH "/tmp/uds-roundtrip.sock"
#define S2C       "server-to-client"
#define C2S       "client-to-server"

static int read_full(int fd, char *buf, size_t want) {
    size_t got = 0;
    while (got < want) {
        ssize_t r = read(fd, buf + got, want - got);
        if (r <= 0) return -1;
        got += (size_t)r;
    }
    return 0;
}

static int run_server(void) {
    unlink(SOCK_PATH);

    int s = socket(AF_UNIX, SOCK_STREAM, 0);
    if (s < 0) { perror("server: socket"); return 1; }

    struct sockaddr_un sa = {0};
    sa.sun_family = AF_UNIX;
    strncpy(sa.sun_path, SOCK_PATH, sizeof(sa.sun_path) - 1);

    if (bind(s, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        perror("server: bind"); return 1;
    }
    if (listen(s, 1) < 0) { perror("server: listen"); return 1; }

    int c = accept(s, NULL, NULL);
    if (c < 0) { perror("server: accept"); return 1; }

    /* Server writes first. */
    if (write(c, S2C, strlen(S2C)) != (ssize_t)strlen(S2C)) {
        perror("server: write"); return 1;
    }

    char buf[64] = {0};
    if (read_full(c, buf, strlen(C2S)) < 0) {
        fprintf(stderr, "[server] FAIL: short read from client\n");
        return 1;
    }
    if (memcmp(buf, C2S, strlen(C2S)) != 0) {
        fprintf(stderr, "[server] FAIL: got '%.*s' expected '%s'\n",
                (int)strlen(C2S), buf, C2S);
        return 1;
    }

    close(c);
    close(s);
    unlink(SOCK_PATH);
    fprintf(stderr, "[server] PASS\n");
    return 0;
}

static int run_client(void) {
    sleep(1);

    int s = socket(AF_UNIX, SOCK_STREAM, 0);
    if (s < 0) { perror("client: socket"); return 1; }

    struct sockaddr_un sa = {0};
    sa.sun_family = AF_UNIX;
    strncpy(sa.sun_path, SOCK_PATH, sizeof(sa.sun_path) - 1);

    if (connect(s, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        perror("client: connect"); return 1;
    }

    char buf[64] = {0};
    if (read_full(s, buf, strlen(S2C)) < 0) {
        fprintf(stderr, "[client] FAIL: short read from server\n");
        return 1;
    }
    if (memcmp(buf, S2C, strlen(S2C)) != 0) {
        fprintf(stderr, "[client] FAIL: got '%.*s' expected '%s'\n",
                (int)strlen(S2C), buf, S2C);
        return 1;
    }

    if (write(s, C2S, strlen(C2S)) != (ssize_t)strlen(C2S)) {
        perror("client: write"); return 1;
    }

    close(s);
    fprintf(stderr, "[client] PASS\n");
    return 0;
}

int main(void) {
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }
    if (pid == 0) {
        return run_client();
    }
    int srv_rc = run_server();
    int status = 0;
    waitpid(pid, &status, 0);
    int cli_rc = WIFEXITED(status) ? WEXITSTATUS(status) : 1;
    return (srv_rc | cli_rc) ? 1 : 0;
}
