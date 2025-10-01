#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/select.h>
#include <string.h>
#include <errno.h>
#include <sys/wait.h>

int main(void) {
    int fds[2];
    if (pipe(fds) < 0) {
        perror("pipe");
        exit(1);
    }

    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        exit(1);
    }

    if (pid == 0) {
        close(fds[1]); // close write end
        fd_set readfds;
        char buf[128];

        while (1) {
            FD_ZERO(&readfds);
            FD_SET(fds[0], &readfds);

            printf("[child] waiting for data...\n");
            int ret = select(fds[0] + 1, &readfds, NULL, NULL, NULL);
            if (ret < 0) {
                perror("select");
                exit(1);
            }

            if (FD_ISSET(fds[0], &readfds)) {
                int n = read(fds[0], buf, sizeof(buf) - 1);
                if (n <= 0) {
                    if (n == 0) {
                        printf("[child] pipe closed\n");
                        exit(0);
                    }
                    perror("read");
                    exit(1);
                }
                buf[n] = '\0';
                printf("[child] got data: %s\n", buf);
            }
        }
    } else {
        // Parent: write data to pipe
        close(fds[0]); // Close read end
        const char *msg = "hello select!\n";
        sleep(1);
        printf("[parent] writing message\n");
        if (write(fds[1], msg, strlen(msg)) < 0) {
            perror("write");
            exit(1);
        }
        sleep(1);
        close(fds[1]);
        wait(NULL);
    }

    return 0;
}
