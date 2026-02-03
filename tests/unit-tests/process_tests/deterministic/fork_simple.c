#include <assert.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main()
{
    pid_t cpid;
    cpid = fork();
    assert(cpid >= 0);
    
    if (cpid == 0) {
        exit(0);           /* terminate child */
    } else {
        int status;
        pid_t waited_pid = waitpid(cpid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    return 0;
}

