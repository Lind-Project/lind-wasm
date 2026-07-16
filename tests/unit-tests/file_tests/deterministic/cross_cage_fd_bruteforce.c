#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>

int main() {
    for(int i = 3; i < 1024; i++) {
        int r = fcntl(i, F_GETFD);
        assert(r == -1);
        assert(errno == EBADF);
    }
    printf("cross_cage_fd_bruteforce test: PASS\n");
    return 0;
}