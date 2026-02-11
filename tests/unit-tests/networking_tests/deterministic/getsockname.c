#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <arpa/inet.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <unistd.h>

int main() {
    int sockfd;
    struct sockaddr_in addr;
    socklen_t addrlen;
    char ipstr[INET_ADDRSTRLEN];

    // Create socket
    sockfd = socket(AF_INET, SOCK_STREAM, 0);
    assert(sockfd >= 0);

    // Bind to INADDR_ANY with ephemeral port
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_ANY);
    addr.sin_port = 0;
    int ret = bind(sockfd, (struct sockaddr *)&addr, sizeof(addr));
    assert(ret == 0);

    // Get socket name
    memset(&addr, 0, sizeof(addr));
    addrlen = sizeof(addr);
    ret = getsockname(sockfd, (struct sockaddr *)&addr, &addrlen);
    assert(ret == 0);

    // Convert IP to string and check
    ret = inet_ntop(AF_INET, &addr.sin_addr, ipstr, sizeof(ipstr)) != NULL;
    assert(ret);

    // Ensure ephemeral port assigned
    assert(ntohs(addr.sin_port) != 0);

    // Close socket
    ret = close(sockfd);
    assert(ret == 0);

    return 0;
}
