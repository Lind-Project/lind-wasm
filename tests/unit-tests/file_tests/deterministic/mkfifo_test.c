#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

/*
 * FIFO (named pipe) unit test: validates mkfifo, open, read, write.
 * Uses fork so reader and writer run in the same process tree.
 *
 * This exercises the core FIFO path used by lmbench's lat_fifo.
 */

#define FIFO_PATH "/tmp/test_fifo"
#define MSG "Hello FIFO from lind-wasm"

int main(void)
{
    int ret;
    struct stat st;
    pid_t pid;
    int status;

    /* Clean up any leftover FIFO */
    unlink(FIFO_PATH);

    /* Test 1: Create a FIFO */
    ret = mkfifo(FIFO_PATH, 0666);
    if (ret != 0) {
        fprintf(stderr, "mkfifo failed: %s\n", strerror(errno));
    }
    assert(ret == 0);

    /* Test 2: Verify it's a FIFO via stat */
    ret = stat(FIFO_PATH, &st);
    assert(ret == 0);
    assert(S_ISFIFO(st.st_mode));

    /* Test 3: Duplicate mkfifo should fail with EEXIST */
    ret = mkfifo(FIFO_PATH, 0666);
    assert(ret == -1);
    assert(errno == EEXIST);

    /* Test 4: Fork-based read/write through FIFO */
    pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        /* Child: writer */
        int fd = open(FIFO_PATH, O_WRONLY);
        if (fd < 0) {
            fprintf(stderr, "child open(O_WRONLY) failed: %s\n", strerror(errno));
            _exit(1);
        }
        ssize_t n = write(fd, MSG, strlen(MSG));
        assert(n == (ssize_t)strlen(MSG));
        close(fd);
        _exit(0);
    }

    /* Parent: reader */
    int fd = open(FIFO_PATH, O_RDONLY);
    if (fd < 0) {
        fprintf(stderr, "parent open(O_RDONLY) failed: %s\n", strerror(errno));
        return 1;
    }

    char buf[256] = {0};
    ssize_t total = 0;
    while (total < (ssize_t)strlen(MSG)) {
        ssize_t n = read(fd, buf + total, sizeof(buf) - (size_t)total);
        if (n <= 0) break;
        total += n;
    }
    assert(total == (ssize_t)strlen(MSG));
    assert(memcmp(buf, MSG, strlen(MSG)) == 0);
    close(fd);

    /* Wait for child */
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);

    /* Test 5: Cleanup - unlink the FIFO */
    ret = unlink(FIFO_PATH);
    assert(ret == 0);

    /* Verify it's gone */
    ret = stat(FIFO_PATH, &st);
    assert(ret == -1);
    assert(errno == ENOENT);

    printf("All FIFO tests passed\n");
    return 0;
}
