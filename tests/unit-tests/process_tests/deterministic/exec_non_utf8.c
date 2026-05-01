#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

/*
 * Test that execve handles non-UTF-8 bytes in argv/envp without EFAULT.
 * Regression test for get_cstr rejecting non-UTF-8 via CStr::to_str().
 *
 * Forks a child that execs /bin/sh with a non-UTF-8 byte in an
 * environment variable. Before the fix, this returned EFAULT.
 */

int main(void)
{
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        /* Set an env var with non-UTF-8 bytes (0xFF, 0xA7) */
        setenv("TEST_NONASCII", "\xff\xa7", 1);

        char *argv[] = { "/bin/sh", "-c", "exit 0", NULL };
        execv("/bin/sh", argv);
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
