#include <assert.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main()
{
    const char *filename = "forkandopen_test.txt";
    const char *parent_str = "PARENT\n";
    const char *child_str = "CHILD\n";
    
    int fd = open(filename, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    assert(fd >= 0);
    
    ssize_t written = write(fd, parent_str, strlen(parent_str));
    assert(written == (ssize_t)strlen(parent_str));
    
    pid_t pid = fork();
    assert(pid >= 0);
    
    if (pid == 0) {
        int child_fd = open(filename, O_WRONLY | O_APPEND);
        assert(child_fd >= 0);
        ssize_t child_written = write(child_fd, child_str, strlen(child_str));
        assert(child_written == (ssize_t)strlen(child_str));
        close(child_fd);
        exit(0);
    } else {
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
        
        close(fd);
        
        int read_fd = open(filename, O_RDONLY);
        assert(read_fd >= 0);
        
        char buf[32];
        ssize_t bytes_read = read(read_fd, buf, sizeof(buf) - 1);
        assert(bytes_read > 0);
        buf[bytes_read] = '\0';
        
        const char *expected = "PARENT\nCHILD\n";
        assert(strcmp(buf, expected) == 0);
        
        close(read_fd);
    }
    
    return 0;
} 
