#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/epoll.h>
#include <fcntl.h>
#include <string.h>

void test_basic_creation() {
    printf("[TEST] Basic epoll_create1(0)... ");
    
    int fd = epoll_create1(0);
    if (fd < 0) {
        printf("FAILED (errno=%d: %s)\n", errno, strerror(errno));
        exit(1);
    }
    
    printf("PASSED (fd=%d)\n", fd);
    close(fd);
}

void test_cloexec_flag() {
    printf("[TEST] epoll_create1(EPOLL_CLOEXEC)... ");

    int fd = epoll_create1(EPOLL_CLOEXEC);
    if (fd < 0) {
        printf("FAILED (errno=%d: %s)\n", errno, strerror(errno));
        exit(1);
    }

    // Verify the flag was actually set on the file descriptor
    int flags = fcntl(fd, F_GETFD);
    if (flags < 0) {
        perror("fcntl failed");
        close(fd);
        exit(1);
    }
    printf("\nTesting %d %d",EPOLL_CLOEXEC,FD_CLOEXEC);
    if (flags & FD_CLOEXEC) {
        printf("PASSED (FD_CLOEXEC bit is set)\n");
    } else {
        printf("FAILED (FD created, but FD_CLOEXEC missing)\n");
        exit(1);
    }
    close(fd);
}

void test_invalid_flags() {
    printf("[TEST] epoll_create1(INVALID_FLAG)... ");

    // Create a flag that definitely includes bits other than EPOLL_CLOEXEC
    // ~EPOLL_CLOEXEC gives us all bits EXCEPT the valid one.
    unsigned int invalid_flag = 0xffffffff; 

    int fd = epoll_create1(invalid_flag);
    
    if (fd != -1) {
        printf("FAILED (Expected -1, got fd=%d)\n", fd);
        close(fd);
        exit(1);
    }

    if (errno == EINVAL) {
        printf("PASSED (Correctly returned EINVAL)\n");
    } else {
        printf("FAILED (Expected EINVAL, got errno=%d: %s)\n", errno, strerror(errno));
        exit(1);
    }
}

int main() {
    printf("Running epoll_create1 tests...\n");
    printf("-------------------------------\n");

    test_basic_creation();
    test_cloexec_flag();
    test_invalid_flags();

    printf("-------------------------------\n");
    printf("All tests passed successfully.\n");
    return 0;
}
