#include <assert.h>
#include <stdlib.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <unistd.h>

int main()
{
    int pipefd[2];
    assert(pipe(pipefd) == 0);

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        // Child: create fd_set and call select
        close(pipefd[1]);

        fd_set readfds;
        fd_set writefds;
        fd_set exceptfds;

        FD_ZERO(&readfds);
        FD_ZERO(&writefds);
        FD_ZERO(&exceptfds);
        FD_SET(pipefd[0], &readfds);

        struct timeval timeout;
        timeout.tv_sec = 0;
        timeout.tv_usec = 100000;

        int ret = select(pipefd[0] + 1, &readfds, &writefds, &exceptfds,
                         &timeout);
        assert(ret >= 0);

        close(pipefd[0]);
        exit(0);
    } else {
        // Parent
        close(pipefd[0]);

        fd_set writefds;
        FD_ZERO(&writefds);
        FD_SET(pipefd[1], &writefds);

        struct timeval timeout;
        timeout.tv_sec = 0;
        timeout.tv_usec = 100000;

        int ret = select(pipefd[1] + 1, NULL, &writefds, NULL, &timeout);
        assert(ret >= 0);

        close(pipefd[1]);

        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    return 0;
}
