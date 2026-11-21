#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define INPUT_FILE "testfiles/copy_file_range_unit_in.txt"
#define OUTPUT_FILE "testfiles/copy_file_range_unit_out.txt"

static void cleanup(void)
{
    unlink(INPUT_FILE);
    unlink(OUTPUT_FILE);
}

int main(void)
{
    cleanup();

    int in = open(INPUT_FILE, O_CREAT | O_RDWR | O_TRUNC, 0644);
    int out = open(OUTPUT_FILE, O_CREAT | O_RDWR | O_TRUNC, 0644);

    if (in < 0 || out < 0) {
        perror("open");
        cleanup();
        return 1;
    }

    const char *msg = "0123456789";
    size_t len = strlen(msg);

    if (write(in, msg, len) != (ssize_t)len) {
        perror("write");
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    long long in_off = 2;
    long long out_off = 0;

    ssize_t copied = copy_file_range(in, &in_off, out, &out_off, 4, 0);
    if (copied != 4) {
        fprintf(stderr, "copy_file_range copied %d bytes, expected 4\n", (int)copied);
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    if (in_off != 6 || out_off != 4) {
        fprintf(stderr, "unexpected offsets: in_off=%lld out_off=%lld\n", in_off, out_off);
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    char buf[16] = {0};

    if (lseek(out, 0, SEEK_SET) < 0) {
        perror("lseek");
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    ssize_t bytes_read = read(out, buf, sizeof(buf) - 1);
    if (bytes_read < 0) {
        perror("read");
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    if (strcmp(buf, "2345") != 0) {
        fprintf(stderr, "unexpected copied content: '%s'\n", buf);
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    errno = 0;
    copied = copy_file_range(in, &in_off, out, &out_off, 1, 1);
    if (copied != -1 || errno != EINVAL) {
        fprintf(stderr, "nonzero flags should fail with EINVAL, got copied=%d errno=%d\n",
                (int)copied, errno);
        close(in);
        close(out);
        cleanup();
        return 1;
    }

    close(in);
    close(out);
    cleanup();

    printf("copy_file_range unit test passed\n");
    fflush(stdout);

    return 0;
}
