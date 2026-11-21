/* Test case for cross-process semaphore synchronization
* This test will create a semaphore in shared memory,
* fork a child process, and ensure that the child waits
* on the semaphore until the parent posts it, demonstrating
* cross-process synchronization.
*/
#include <assert.h>
#include <errno.h>
#include <semaphore.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
    sem_t *sem = mmap(
        NULL,
        sizeof(*sem),
        PROT_READ | PROT_WRITE,
        MAP_SHARED | MAP_ANONYMOUS,
        -1,
        0
    );

    assert(sem != MAP_FAILED);

    // pshared=1 and initial value=0 force cross-process synchronization.
    assert(sem_init(sem, 1, 0) == 0);

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        printf("[child] waiting\n");
        fflush(stdout);

        assert(sem_wait(sem) == 0);

        printf("[child] awakened\n");
        fflush(stdout);
        _exit(0);
    }

    sleep(1);

    printf("[parent] posting\n");
    fflush(stdout);

    assert(sem_post(sem) == 0);

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);

    assert(sem_destroy(sem) == 0);
    assert(munmap(sem, sizeof(*sem)) == 0);

    printf("[parent] done\n");
    return 0;
}
