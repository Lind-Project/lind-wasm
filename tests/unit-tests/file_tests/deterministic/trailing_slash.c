#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>

static const char *regular_path = "trailing_slash_regular_file";
static const char *regular_path_slash = "trailing_slash_regular_file/";

static void fail(const char *msg) {
    perror(msg);
    unlink(regular_path);
    _exit(1);
}

static void expect_trailing_slash_failure(const char *name, int ret) {
    if (ret != -1) {
        fprintf(stderr, "%s unexpectedly succeeded\n", name);
        unlink(regular_path);
        _exit(1);
    }
}

int main(void) {
    struct stat st;
    struct timespec ts[2] = {
        { .tv_sec = 123, .tv_nsec = 0 },
        { .tv_sec = 456, .tv_nsec = 0 },
    };

    unlink(regular_path);

    int fd = open(regular_path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        fail("create regular file");
    }
    close(fd);

    errno = 0;
    fd = open(regular_path_slash, O_RDONLY);
    expect_trailing_slash_failure("open regular file with trailing slash", fd);

    errno = 0;
    expect_trailing_slash_failure(
        "stat regular file with trailing slash",
        stat(regular_path_slash, &st)
    );

    errno = 0;
    expect_trailing_slash_failure(
        "utimensat regular file with trailing slash",
        utimensat(AT_FDCWD, regular_path_slash, ts, 0)
    );

    unlink(regular_path);
    printf("trailing slash checks passed\n");
    return 0;
}
