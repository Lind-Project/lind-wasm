#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

/*
 * Test that execve handles non-UTF-8 bytes in argv without EFAULT.
 * Regression test for get_cstr rejecting non-UTF-8 via CStr::to_str().
 *
 * The child execs itself with a marker arg so we know it ran,
 * plus an arg containing raw 0xFF and 0xA7 bytes.
 */

int main(int argc, char *argv[])
{
    if (argc >= 2 && strcmp(argv[1], "--execd") == 0) {
        /* We are the exec'd child. Verify the non-UTF-8 arg survived. */
        assert(argc >= 3);
        unsigned char *p = (unsigned char *)argv[2];
        assert(p[0] == 0xFF);
        assert(p[1] == 0xA7);
        assert(p[2] == 'x');
        assert(p[3] == '\0');
        printf("exec_non_utf8: child received non-UTF-8 arg OK\n");
        return 0;
    }

    /* Parent: fork and exec ourselves with non-UTF-8 argv. */
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        char non_utf8_arg[] = { (char)0xFF, (char)0xA7, 'x', '\0' };
        char *new_argv[] = { argv[0], "--execd", non_utf8_arg, NULL };
        execv(argv[0], new_argv);
        /* If exec fails, exit with error */
        perror("execv");
        _exit(1);
    }

    int status;
    pid_t w = waitpid(pid, &status, 0);
    assert(w >= 0);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);

    printf("exec_non_utf8: all tests passed\n");
    return 0;
}
