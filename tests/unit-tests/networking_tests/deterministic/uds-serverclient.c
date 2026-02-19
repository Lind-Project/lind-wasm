#include <unistd.h>
#include <stdio.h>
#include <sys/socket.h>
#include <stdlib.h>
#include <string.h>
#include <sys/un.h>
#include <sys/wait.h>

#define MSG "UDS_ECHO_TEST"
#define MSG_LEN 13

static void client_run(const char *path)
{
    int fd;
    struct sockaddr_un addr;
    char buf[64];
    ssize_t n;

    fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        perror("client socket");
        exit(1);
    }
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, path, sizeof(addr.sun_path) - 1);
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        perror("client connect");
        close(fd);
        exit(1);
    }
    n = send(fd, MSG, MSG_LEN, 0);
    if (n != MSG_LEN) {
        close(fd);
        exit(1);
    }
    n = recv(fd, buf, sizeof(buf), 0);
    if (n != MSG_LEN || memcmp(buf, MSG, MSG_LEN) != 0) {
        close(fd);
        exit(1);
    }
    close(fd);
    exit(0);
}

int main(void)
{
    int server_fd, client_fd;
    struct sockaddr_un addr;
    socklen_t addrlen;
    char path[64];
    char buf[64];
    ssize_t n;
    pid_t pid;
    int wstatus;

    snprintf(path, sizeof(path), "uds_%d.sock", getpid());
    unlink(path);

    server_fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (server_fd < 0) {
        perror("server socket");
        unlink(path);
        return 1;
    }
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, path, sizeof(addr.sun_path) - 1);
    if (bind(server_fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        perror("server bind");
        close(server_fd);
        unlink(path);
        return 1;
    }
    if (listen(server_fd, 1) < 0) {
        perror("server listen");
        close(server_fd);
        unlink(path);
        return 1;
    }

    pid = fork();
    if (pid < 0) {
        perror("fork");
        close(server_fd);
        unlink(path);
        return 1;
    }
    if (pid == 0) {
        close(server_fd);
        client_run(path);
    }

    addrlen = sizeof(addr);
    client_fd = accept(server_fd, (struct sockaddr *)&addr, &addrlen);
    if (client_fd < 0) {
        perror("accept");
        close(server_fd);
        unlink(path);
        return 1;
    }
    n = recv(client_fd, buf, sizeof(buf), 0);
    if (n != MSG_LEN) {
        close(client_fd);
        close(server_fd);
        unlink(path);
        return 1;
    }
    n = send(client_fd, buf, (size_t)n, 0);
    if (n != MSG_LEN) {
        close(client_fd);
        close(server_fd);
        unlink(path);
        return 1;
    }
    close(client_fd);
    close(server_fd);
    if (waitpid(pid, &wstatus, 0) != pid || !WIFEXITED(wstatus) || WEXITSTATUS(wstatus) != 0) {
        unlink(path);
        return 1;
    }
    unlink(path);
    return 0;
}
