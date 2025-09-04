#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>

// This file is used to test four functionality that has been handled by
// RawPOSIX: F_DUPFD, F_DUPFD_CLOEXEC, F_GETFD, and F_SETFD

void check_fd_flags(int fd, const char *msg) {
    int flags = fcntl(fd, F_GETFD);
    if (flags == -1) {
        perror(msg);
        exit(EXIT_FAILURE);
    }
    // fd flag: 0 means not set (false) and 1 means set (true)
    printf("%s: FD flags = 0x%x\n", msg, flags);
}

int main() {
    const char *filename = "fcntltestfile.txt";
    int fd = open(filename, O_CREAT | O_RDWR, 0644);
    if (fd == -1) {
        perror("open");
        exit(EXIT_FAILURE);
    }

    printf("Original FD: %d\n", fd);

    // Test F_DUPFD with arg=100, ideally the newfd should be 101
    int newfd = fcntl(fd, F_DUPFD, 100);
    if (newfd == -1) {
        perror("F_DUPFD failed");
        close(fd);
        exit(EXIT_FAILURE);
    }

    printf("F_DUPFD duplicated FD: %d\n", newfd);

    // Test F_DUPFD_CLOEXEC with arg=100, ideally the newfd should be 101
    int newfd_cloexec = fcntl(fd, F_DUPFD_CLOEXEC, 100);
    if (newfd_cloexec == -1) {
        perror("F_DUPFD_CLOEXEC failed");
        close(fd);
        close(newfd);
        exit(EXIT_FAILURE);
    }
    printf("F_DUPFD_CLOEXEC duplicated FD: %d\n", newfd_cloexec);

    // Check F_GETFD on original FD
    check_fd_flags(fd, "Original FD flags");

    // Check F_GETFD on newfd
    check_fd_flags(newfd, "F_DUPFD duplicated FD flags");

    // Check F_GETFD on newfd_cloexec
    check_fd_flags(newfd_cloexec, "F_DUPFD_CLOEXEC duplicated FD flags");

    // Set FD_CLOEXEC on newfd
    if (fcntl(newfd, F_SETFD, FD_CLOEXEC) == -1) {
        perror("F_SETFD failed");
        close(fd);
        close(newfd);
        close(newfd_cloexec);
        exit(EXIT_FAILURE);
    }

    // Check again after setting FD_CLOEXEC
    check_fd_flags(newfd, "After F_SETFD on F_DUPFD duplicated FD");

    // Clean up
    close(fd);
    close(newfd);
    close(newfd_cloexec);
    unlink(filename);

    printf("Test completed successfully.\n");
    return EXIT_SUCCESS;
}
