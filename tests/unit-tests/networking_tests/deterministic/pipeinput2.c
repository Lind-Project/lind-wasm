#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
    const char *test_msg = "Hey Nick!\n";
    const size_t test_msg_len = strlen(test_msg);
    int pipefd[2];
    pid_t cpid;
    int ret;
    int status;

    ret = pipe(pipefd);
    assert(ret == 0);

    cpid = fork();
    assert(cpid >= 0);

    if (cpid == 0) {
        /* Child reads from pipe */
        ret = close(pipefd[1]);
        assert(ret == 0);

        char read_buf[test_msg_len];
        char buf[1];
        size_t total_read = 0;
        while (total_read < test_msg_len) {
            ret = read(pipefd[0], buf, 1);
            assert(ret == 1);
            read_buf[total_read] = buf[0];
            total_read++;
        }
        assert(total_read == test_msg_len);
        assert(memcmp(read_buf, test_msg, test_msg_len) == 0);

        ret = close(pipefd[0]);
        assert(ret == 0);

        exit(0);
    } else {
        /* Parent writes to pipe */
        ret = close(pipefd[0]);
        assert(ret == 0);

        ret = write(pipefd[1], test_msg, test_msg_len);
        assert(ret == (int)test_msg_len);

        ret = close(pipefd[1]);
        assert(ret == 0);

        pid_t waited_pid = waitpid(cpid, &status, 0);
        assert(waited_pid == cpid);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    return 0;
}

