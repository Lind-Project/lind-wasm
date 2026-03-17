#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <sys/socket.h>

#define PORT 12345
#define BUFSIZE 1024

int main(int argc, char *argv[]) {
    int sockfd;
    struct sockaddr_in servaddr, cliaddr;
    char buffer[BUFSIZE];

    sockfd = socket(AF_INET, SOCK_DGRAM, 0);
    if (sockfd < 0) {
        perror("socket");
        exit(1);
    }

    if (argc > 1 && strcmp(argv[1], "server") == 0) {
        // server
        memset(&servaddr, 0, sizeof(servaddr));
        servaddr.sin_family = AF_INET;
        servaddr.sin_addr.s_addr = INADDR_ANY;
        servaddr.sin_port = htons(PORT);

        if (bind(sockfd, (struct sockaddr*)&servaddr, sizeof(servaddr)) < 0) {
            perror("bind");
            exit(1);
        }

        socklen_t len = sizeof(cliaddr);
        int n = recvfrom(sockfd, buffer, BUFSIZE - 1, 0,
                         (struct sockaddr*)&cliaddr, &len);
        if (n < 0) {
            perror("recvfrom");
            exit(1);
        }

        buffer[n] = '\0';
        printf("Server received: %s\n", buffer);
    } else {
        // client
        memset(&servaddr, 0, sizeof(servaddr));
        servaddr.sin_family = AF_INET;
        servaddr.sin_port = htons(PORT);
        servaddr.sin_addr.s_addr = inet_addr("127.0.0.1");

        const char *msg = "Hello recvfrom!";
        if (sendto(sockfd, msg, strlen(msg), 0,
                   (struct sockaddr*)&servaddr, sizeof(servaddr)) < 0) {
            perror("sendto");
            exit(1);
        }

        printf("Client sent: %s\n", msg);
    }

    close(sockfd);
    return 0;
}
