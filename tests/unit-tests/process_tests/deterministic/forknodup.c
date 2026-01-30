#include <assert.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main()
{
    const char *filename = "forknodup_test.txt";
    const char *payload = "ABCDEF";
    size_t payload_len = 6;
    
    int fd = open(filename, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    
    ssize_t written = write(fd, payload, payload_len);
    assert(written == (ssize_t)payload_len);
    
    off_t seek_result = lseek(fd, 0, SEEK_SET);
    assert(seek_result == 0);
    
    pid_t pid = fork();
    assert(pid >= 0);
    
    if (pid == 0) {
        int close_result = close(fd);
        assert(close_result == 0);
        exit(0);
    } else {
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
        
        char readbuf[10];
        ssize_t numread = read(fd, readbuf, payload_len);
        assert(numread == (ssize_t)payload_len);
        assert(memcmp(readbuf, payload, payload_len) == 0);
        
        int close_result = close(fd);
        assert(close_result == 0);
    }
    
    return 0;
}
