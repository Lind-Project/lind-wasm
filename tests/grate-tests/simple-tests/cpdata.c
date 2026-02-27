// Tests copy_data_between_cages by writing a string through an interposed
// write() syscall.  The grate intercepts write(), copies the buffer from the
// cage into a malloc'd destination, and verifies the contents.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <assert.h>

int main(int argc, char *argv[]) {
    const char *msg = "hello";
    // write() to stdout — the grate intercepts this and validates via copy_data
    ssize_t ret = write(STDOUT_FILENO, msg, strlen(msg));
    if (ret < 0) {
        perror("write");
        assert(0);
    }
    printf("[Cage | cpdata] PASS: write returned %zd\n", ret);
    return 0;
}
