#define _GNU_SOURCE
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <assert.h>
#include <errno.h>

int main() {
    // open file for writing
    int fd1 = open("dup3_test.txt", O_CREAT | O_WRONLY | O_TRUNC, 0644);
    assert(fd1 >= 0);

    // duplicate fd1 to fd2 with O_CLOEXEC
    int fd2 = dup3(fd1, fd1 + 1, O_CLOEXEC);
    assert(fd2 > fd1);

    // check FD_CLOEXEC flag
    int flags = fcntl(fd2, F_GETFD);
    assert(flags & FD_CLOEXEC);

    // write to both descriptors
    write(fd1, "A", 1);
    write(fd2, "B", 1);

    // check file content is "AB"
    close(fd1);
    close(fd2);
    FILE *f = fopen("dup3_test.txt", "r");
    char buf[4] = {0};
    fread(buf, 1, 3, f);
    fclose(f);
    assert(buf[0] == 'A' && buf[1] == 'B');

    printf("dup3 basic test passed.\n");
    return 0;
}



