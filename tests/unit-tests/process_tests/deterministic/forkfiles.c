#include <assert.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main()
{
    const char *filename = "forkfiles_test.txt";
    const char *payload = "HELLO_FORKFILES";
    size_t payload_len = strlen(payload);
    
    int fd = open(filename, O_CREAT | O_TRUNC | O_RDWR, 0644);
    assert(fd >= 0);
    
    ssize_t written = write(fd, payload, payload_len);
    assert(written == (ssize_t)payload_len);
    
    off_t seek_result = lseek(fd, 0, SEEK_SET);
    assert(seek_result == 0);
    
    pid_t pid = fork();
    assert(pid >= 0);
    
    if (pid == 0) {
        close(fd);
        exit(0);
    } else {
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
        
        char readbuf[100];
        ssize_t numread = read(fd, readbuf, sizeof(readbuf) - 1);
        assert(numread == (ssize_t)payload_len);
        readbuf[numread] = '\0';
        
        assert(memcmp(readbuf, payload, payload_len) == 0);
        
        close(fd);
    }
    
    return 0;
}
