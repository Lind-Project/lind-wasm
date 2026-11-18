#include <assert.h>
#include <errno.h>
#include <sys/wait.h>
#include <unistd.h>

/* Use TOKIO_WORKER_THREADS=1 and a six-task container limit when running via timeout. */
int main(void) {
    errno = 0;
    assert(fork() == -1);
    assert(errno == EAGAIN);

    int status;
    errno = 0;
    assert(waitpid(-1, &status, WNOHANG) == -1);
    assert(errno == ECHILD);
    return 0;
}
