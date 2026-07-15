/* Test case for cross-process semaphore synchronization 
* This test will create a semaphore in shared memory, 
* fork a child process, and ensure that the child waits 
* on the semaphore until the parent posts it, demonstrating 
* cross-process synchronization.
*/
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

    if (sem == MAP_FAILED) {
        perror("mmap");
        return 1;
    }

    // pshared=1 and initial value=0 force cross-process synchronization.
    if (sem_init(sem, 1, 0) != 0) {
        perror("sem_init");
        return 1;
    }

    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        return 1;
    }

    if (pid == 0) {
        printf("[child] waiting\n");
        fflush(stdout);

        if (sem_wait(sem) != 0) {
            perror("sem_wait");
            _exit(1);
        }

        printf("[child] awakened\n");
        fflush(stdout);
        _exit(0);
    }

    sleep(1);

    printf("[parent] posting\n");
    fflush(stdout);

    if (sem_post(sem) != 0) {
        perror("sem_post");
        return 1;
    }

    waitpid(pid, NULL, 0);
    sem_destroy(sem);
    munmap(sem, sizeof(*sem));

    printf("[parent] done\n");
    return 0;
}
