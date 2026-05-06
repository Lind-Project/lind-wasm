/*
 * AF_UNIX postmaster-pattern test.
 *
 * Mirrors how PostgreSQL's postmaster handles a new connection:
 *   - parent: socket / bind / listen / accept → fork → close its copy of
 *     the accepted fd → wait for child.
 *   - child  (backend equivalent): write a server tag, read client's tag,
 *     close.
 *
 * On the client side: connect, read server tag, write client tag, close.
 *
 * Catches bugs in IPC-fd refcounting / inheritance across fork():
 *   - if the child's inherited connected fd loses a ref when parent closes,
 *     the child's write fails with EPIPE.
 *   - if pipe refs aren't bumped at fork time, peers see immediate EOF.
 *
 * Passes natively (kernel UDS) and should pass under any correct
 * implementation that emulates UDS via cross-cage pipes.
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

#define SOCK_PATH "/tmp/uds-postmaster-pattern.sock"
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

/* Body of the "backend" child: I/O on the inherited connected fd. */
static int run_backend(int conn_fd) {
    if (write(conn_fd, S2C, strlen(S2C)) != (ssize_t)strlen(S2C)) {
        perror("backend: write");
        return 1;
    }

    char buf[64] = {0};
    if (read_full(conn_fd, buf, strlen(C2S)) < 0) {
        fprintf(stderr, "[backend] FAIL: short read from client\n");
        return 1;
    }
    if (memcmp(buf, C2S, strlen(C2S)) != 0) {
        fprintf(stderr, "[backend] FAIL: bad client tag\n");
        return 1;
    }

    close(conn_fd);
    fprintf(stderr, "[backend] PASS\n");
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

    /* postmaster pattern: fork a backend, then close the accepted fd
     * in the parent so the child owns it. */
    pid_t backend = fork();
    if (backend < 0) { perror("server: fork"); return 1; }
    if (backend == 0) {
        close(s);  /* backend doesn't need the listening socket */
        _exit(run_backend(c));
    }

    /* Parent (postmaster equivalent) closes its copy. */
    close(c);

    int status = 0;
    waitpid(backend, &status, 0);
    int rc = WIFEXITED(status) ? WEXITSTATUS(status) : 1;

    close(s);
    unlink(SOCK_PATH);
    return rc;
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
        fprintf(stderr, "[client] FAIL: bad server tag\n");
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
