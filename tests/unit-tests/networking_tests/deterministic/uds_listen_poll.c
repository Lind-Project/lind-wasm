/*
 * AF_UNIX listen-fd readiness test.
 *
 * Single binary forks server + client.  Server: bind/listen, select(POLLIN)
 * with a 5s timeout, accept, read.  Client: sleep briefly, connect, write.
 * Passes iff the listening fd becomes readable once a connection is pending.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

#define SOCK_PATH "/tmp/uds-listen-poll-repro.sock"
#define MSG       "ping\n"

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

    fprintf(stderr, "[server] waiting in select() on listen fd %d\n", s);
    fflush(stderr);

    fd_set rfds;
    FD_ZERO(&rfds);
    FD_SET(s, &rfds);
    struct timeval tv = { .tv_sec = 5, .tv_usec = 0 };

    int n = select(s + 1, &rfds, NULL, NULL, &tv);
    if (n == 0) {
        fprintf(stderr, "[server] FAIL: select timed out — "
                        "listen fd never became readable\n");
        return 1;
    }
    if (n < 0) { perror("server: select"); return 1; }
    fprintf(stderr, "[server] select returned %d, accepting\n", n);

    int c = accept(s, NULL, NULL);
    if (c < 0) { perror("server: accept"); return 1; }

    char buf[64] = {0};
    ssize_t r = read(c, buf, sizeof(buf) - 1);
    if (r <= 0) { perror("server: read"); return 1; }

    if (strcmp(buf, MSG) != 0) {
        fprintf(stderr, "[server] FAIL: got '%s' expected '%s'\n", buf, MSG);
        return 1;
    }

    close(c);
    close(s);
    unlink(SOCK_PATH);
    fprintf(stderr, "[server] PASS\n");
    return 0;
}

static int run_client(void) {
    /* Give the parent time to bind+listen before we connect. */
    sleep(1);

    int s = socket(AF_UNIX, SOCK_STREAM, 0);
    if (s < 0) { perror("client: socket"); return 1; }

    struct sockaddr_un sa = {0};
    sa.sun_family = AF_UNIX;
    strncpy(sa.sun_path, SOCK_PATH, sizeof(sa.sun_path) - 1);

    if (connect(s, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        perror("client: connect"); return 1;
    }
    if (write(s, MSG, strlen(MSG)) != (ssize_t)strlen(MSG)) {
        perror("client: write"); return 1;
    }
    close(s);
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
