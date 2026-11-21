#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>

#define B_FD_START 500
#define B_FD_COUNT 10

int main() {
    int to_parent[2];
    int to_child[2];
    assert(pipe(to_parent) == 0);
    assert(pipe(to_child) == 0);
    pid_t pid = fork();
    assert(pid != -1);

    if(pid == 0) {
        // Cage B (child)
        close(to_parent[0]);
        close(to_child[1]);

        for(int i = 0; i < B_FD_COUNT; i++) {
            int rawfd = open("/dev/null", O_RDONLY);
            assert(rawfd >= 0);
            int target = B_FD_START + i;
            assert(dup2(rawfd, target) == target);
            if(rawfd != target) {
                close(rawfd);
            }
        }

        assert(write(to_parent[1], "r", 1) == 1);

        char ack;
        assert(read(to_child[0], &ack, 1) == 1);

        for(int i = 0; i < B_FD_COUNT; i++) {
            assert(fcntl(B_FD_START+i, F_GETFD) != -1);
        }
        for(int i = 0; i < B_FD_COUNT; i++) {
            close(B_FD_START+i);
        }
        close(to_parent[1]);
        close(to_child[0]);
        _exit(0);
    } else {
        // Cage A (parent)
        close(to_parent[1]);
        close(to_child[0]);

        char ready;
        assert(read(to_parent[0], &ready, 1) == 1);

        for(int i = 3; i < 1024; i++) {
            if(i == to_parent[0] || i == to_child[1]) {
                continue;
            }
            int r = fcntl(i, F_GETFD);
            assert(r== -1);
            assert(errno == EBADF);
        }

        char buf[1];
        for(int i = 0; i < B_FD_COUNT; i++) {
            int fd = B_FD_START + i;

            errno = 0;
            assert(fcntl(fd, F_GETFD) == -1);
            assert(errno == EBADF);

            errno = 0; 
            assert(read(fd, buf, 1) == -1);
            assert(errno == EBADF);

            errno = 0;
            assert(close(fd) == -1);
            assert(errno == EBADF);
        }

        assert(write(to_child[1], "d", 1) == 1);
        close(to_parent[0]);
        close(to_child[1]);

        int status;
        assert(waitpid(pid, &status, 0) == pid);
        assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    }

    printf("cross_cage_fd_bruteforce test: PASS\n");
    return 0;
}
