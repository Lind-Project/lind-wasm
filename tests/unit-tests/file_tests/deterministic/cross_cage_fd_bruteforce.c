#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>


int main() {
    for(int i = 3; i < 1024; i++) {
        int r = fcntl(i, F_GETFD);
        assert(r == -1);
        assert(errno == EBADF);
    }

    int to_parent[2];
    int to_child[2];
    assert(pipe(to_parent) == 0);
    assert(pipe(to_child) == 0);

    pid_t pid = fork();
    assert(pid != -1);

    if(pid == 0) {
        close(to_parent[0]);
        close(to_child[1]);

        int rawfd = open("/dev/null", O_RDONLY);
        assert(rawfd >= 0);
        const int B_FD = 500;
        assert(dup2(rawfd, B_FD) == B_FD);
        if(rawfd != B_FD) {
            close(rawfd);
        }
        int msg = B_FD;
        assert(write(to_parent[1], &msg, sizeof(msg)) == sizeof(msg));

        char ack;
        assert(read(to_child[0], &ack, 1) == 1);
        assert(fcntl(B_FD, F_GETFD) != -1);

        close(to_parent[1]);
        close(to_child[0]);
        close(B_FD);
        _exit(0);

        close(to_parent[1]);
        close(to_child[0]);

        int b_fd;
        assert(read(to_parent[0], &b_fd, sizeof(b_fd)) == sizeof(b_fd));

        int r = fcntl(b_fd, F_GETFD);
        assert(r == -1);
        assert(errno == EBADF);

        assert(write(to_child[1], "x", 1) == 1);

        close(to_parent[0]);
        close(to_child[1]);

        int status;
        assert(waitpid(pid, &status, 0) == pid);
        assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    }
    

    printf("cross_cage_fd_bruteforce test: PASS\n");
    return 0;
}