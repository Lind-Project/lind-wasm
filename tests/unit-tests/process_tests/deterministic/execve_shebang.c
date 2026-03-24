#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>
#include <errno.h>
#include <assert.h>

int main() {
    const char *script_invalid = "./test_script_invalid.sh";
    int fd_invalid = open(script_invalid, O_WRONLY | O_CREAT | O_TRUNC, 0755);
    if (fd_invalid < 0) {
        perror("open");
        exit(1);
    }

    const char *content_invalid =
        "automated_tests/hello\n"
        "unreachable section\n";

    write(fd_invalid, content_invalid, strlen(content_invalid));
    close(fd_invalid);

    // Execute the script
    assert(execve(script_invalid, (char *const []){ (char *)script_invalid, NULL }, (char *const []){ NULL }) == -1);

    assert(errno == ENOEXEC);

    return 0;
}
