/*
 * AF_UNIX accept(2) peer-address writeback test.
 *
 * Verifies that when the caller passes a non-NULL addr/addrlen pair to
 * accept(), the kernel/runtime fills the peer sockaddr with sa_family =
 * AF_UNIX and updates addrlen to at least sizeof(sa_family_t).
 *
 * Without this, callers that read accept()'s peer-addr writeback
 * (postgres backend startup, sshd, etc.) see ss_family == 0 and later
 * getnameinfo() calls fail with EAI_FAMILY.
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

#define SOCK_PATH "/tmp/uds-accept-peer-addr.sock"

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

    /* Wait for a connection so accept doesn't block forever on failure. */
    fd_set rfds;
    FD_ZERO(&rfds);
    FD_SET(s, &rfds);
    struct timeval tv = { .tv_sec = 5, .tv_usec = 0 };
    int n = select(s + 1, &rfds, NULL, NULL, &tv);
    if (n <= 0) {
        fprintf(stderr, "[server] select failed (n=%d)\n", n);
        return 1;
    }

    /* Pass a real buffer; we want to assert the runtime writes it. */
    struct sockaddr_storage peer;
    memset(&peer, 0xff, sizeof(peer)); /* sentinel: detect "never written" */
    socklen_t peerlen = sizeof(peer);

    int c = accept(s, (struct sockaddr *)&peer, &peerlen);
    if (c < 0) { perror("server: accept"); return 1; }

    if (peerlen < sizeof(sa_family_t)) {
        fprintf(stderr,
                "[server] FAIL: accept did not update addrlen "
                "(got %u, expected >= %zu)\n",
                (unsigned)peerlen, sizeof(sa_family_t));
        return 1;
    }
    if (peer.ss_family != AF_UNIX) {
        fprintf(stderr,
                "[server] FAIL: peer.ss_family = %u, expected AF_UNIX (%u)\n",
                (unsigned)peer.ss_family, (unsigned)AF_UNIX);
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
    /* Hold the connection open until the server has read its accept result. */
    sleep(1);
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
