#include <unistd.h>
#include <stdio.h>
#include <sys/socket.h>
#include <stdlib.h>
#include <string.h>
#include <sys/un.h>


#define SOCK_PATH "unix_sock.tmp"

int main(int argc, char *argv[])
{
    socklen_t addrlen;
    int rc;
    int server_sock;
    struct sockaddr_un server_addr, server_addr2;

    unlink(SOCK_PATH); // just in case this didnt get unlinked last time

    int server_fd = socket(AF_UNIX, SOCK_STREAM, 0);

    server_addr.sun_family = AF_UNIX;
    strcpy(server_addr.sun_path, SOCK_PATH);
    if (bind(server_fd, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0)
    {
        perror("BIND ERROR: ");
        close(server_fd);
        exit(1);
    }

    addrlen = sizeof(server_addr2);
    rc = getsockname(server_fd, (struct sockaddr *)&server_addr2, &addrlen);

    if (rc == -1)
    {
        perror("GETSOCKNAME ERROR: ");
    }

    const char *p = server_addr2.sun_path;
    const char *root = "/home/lind/lind-wasm/src/tmp/";
    if (strncmp(p, root, strlen(root)) == 0) p += strlen(root);
    printf("sun_path = %s\n", p);

    close(server_fd);
    unlink(SOCK_PATH);
    
    return 0;
}
