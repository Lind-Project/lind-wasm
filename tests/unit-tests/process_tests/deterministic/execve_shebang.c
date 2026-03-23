#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>
#include <errno.h>
#include <assert.h>

int main() {
    const char *script_valid = "./test_script_valid.sh";
    const char *script_invalid = "./test_script_invalid.sh";

    // Create a script with a shebang
    int fd_valid = open(script_valid, O_WRONLY | O_CREAT | O_TRUNC, 0755);
    if (fd_valid < 0) {
        perror("open");
        exit(1);
    }
    int fd_invalid = open(script_invalid, O_WRONLY | O_CREAT | O_TRUNC, 0755);
    if (fd_invalid < 0) {
        perror("open");
        exit(1);
    }

    const char *content_valid =
        "#!automated_tests/hello\n"
        "unreachable section\n";

    write(fd_valid, content_valid, strlen(content_valid));
    close(fd_valid);

    const char *content_invalid =
        "automated_tests/hello\n"
        "unreachable section\n";

    write(fd_invalid, content_invalid, strlen(content_invalid));
    close(fd_invalid);

    // Prepare arguments for execve
    char *const args[] = { (char *)script_valid, NULL };
    char *const env[] = { NULL };

    // Execute the script
    assert(execve(script_invalid, (char *const []){ (char *)script_invalid, NULL }, (char *const []){ NULL }) == -1);

    assert(errno == ENOEXEC);

    // Execute the script
    if (execve(script_valid, (char *const []){ (char *)script_valid, NULL }, (char *const []){ NULL }) == -1) {
        perror("execve");
        exit(1);
    }

    return 0;
}
