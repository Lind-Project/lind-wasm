#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
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
        printf("[Cage] Forked process with PID: %d\n", cpid);
        assert(cpid == 10); // The fork_grate handler in the grate should return 10, which is determined by the grate ret
        int status;
        assert(wait(&status) > 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    return 0;
}
