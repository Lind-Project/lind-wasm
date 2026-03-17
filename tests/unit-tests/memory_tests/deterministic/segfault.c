#include <assert.h>
#include <sys/wait.h>
#include <unistd.h>
#include <string.h>
#include <stdlib.h>

int main(void) {
    int fd[2];
    const char *message = "OK\n";
    const size_t message_len = 3; // strlen("OK\n")
    char buf[4] = {0};
    ssize_t total_read = 0;
    ssize_t n;

    // Create pipe
    assert(pipe(fd) == 0);

    // Fork
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        // Child: close read end
        assert(close(fd[0]) == 0);

        // Write fixed message "OK\n" to fd[1]
        ssize_t written = write(fd[1], message, message_len);
        assert(written == (ssize_t)message_len);

        // Close write end
        assert(close(fd[1]) == 0);

        _exit(0);
    } else {
        // Parent: close write end
        assert(close(fd[1]) == 0);

        // Read exactly len("OK\n") bytes (loop if needed)
        while (total_read < (ssize_t)message_len) {
            n = read(fd[0], buf + total_read, message_len - total_read);
            assert(n > 0);
            total_read += n;
        }
        assert(total_read == (ssize_t)message_len);

        // Assert memcmp == 0
        assert(memcmp(buf, message, message_len) == 0);

        // Close read end
        assert(close(fd[0]) == 0);

        // Wait for child
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    return 0;
}
