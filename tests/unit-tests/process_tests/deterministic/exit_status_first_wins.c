/* Test: exit_syscall must not record the cage exit status. */

#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static void *thread_a(void *arg)
{
    (void)arg;
    return NULL;
}

int main(void)
{
    pid_t pid = fork();
    assert(pid != -1 && "fork should succeed");

    if (pid == 0) {
        pthread_t ta;
        int rc = pthread_create(&ta, NULL, thread_a, NULL);
        assert(rc == 0 && "pthread_create should succeed");
        pthread_join(ta, NULL);
        exit(1);
        _exit(99); /* unreachable */
    }

    int status;
    pid_t waited = waitpid(pid, &status, 0);
    assert(waited == pid && "waitpid should return child pid");
    assert(WIFEXITED(status) && "child should exit normally");
    assert(WEXITSTATUS(status) == 1 &&
           "exit_group(1) should win over thread SYS_exit(0)");

    printf("exit_status_first_wins: PASS\n");
    return 0;
}
