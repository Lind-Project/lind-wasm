#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/ipc.h>
#include <sys/shm.h>
#include <unistd.h>
#include <sys/wait.h>

#define SHM_SIZE 4096  /* Size of shared memory segment */

int main() {
    key_t key = 1234;   /* Shared memory key */
    int shmid;
    char *shmaddr;

    /* Create the shared memory segment with IPC_CREAT flag */
    shmid = shmget(key, SHM_SIZE, IPC_CREAT | 0666);
    if (shmid < 0) {
        perror("shmget");
        exit(EXIT_FAILURE);
    }
    printf("Shared memory segment created with id: %d\n", shmid);

    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        exit(EXIT_FAILURE);
    } else if (pid == 0) {
        // Child process: wait briefly to allow parent to write first.
        sleep(1);
        shmaddr = (char *)shmat(shmid, NULL, 0);
        if (shmaddr == (char *) -1) {
            perror("shmat in child");
            exit(EXIT_FAILURE);
        }
        printf("Child attached to shared memory at %p\n", shmaddr);

        // Read what the parent wrote.
        printf("Child reads: '%s'\n", shmaddr);

        // Write a response into shared memory.
        const char *child_msg = "Hello from child";
        strncpy(shmaddr, child_msg, SHM_SIZE - 1);
        shmaddr[SHM_SIZE - 1] = '\0';

        // Detach shared memory.
        if (shmdt(shmaddr) == -1) {
            perror("shmdt in child");
            exit(EXIT_FAILURE);
        }
        printf("Child detached from shared memory\n");
        exit(EXIT_SUCCESS);
    } else {
        // Parent process.
        shmaddr = (char *)shmat(shmid, NULL, 0);
        if (shmaddr == (char *) -1) {
            perror("shmat in parent");
            exit(EXIT_FAILURE);
        }
        printf("Parent attached to shared memory at %p\n", shmaddr);

        // Write a message to shared memory.
        const char *parent_msg = "Hello from parent";
        strncpy(shmaddr, parent_msg, SHM_SIZE - 1);
        shmaddr[SHM_SIZE - 1] = '\0';
        printf("Parent wrote: '%s'\n", shmaddr);

        // Wait for the child to finish.
        wait(NULL);

        // Read child's message.
        printf("Parent reads: '%s'\n", shmaddr);

        // Detach shared memory.
        if (shmdt(shmaddr) == -1) {
            perror("shmdt in parent");
            exit(EXIT_FAILURE);
        }
        printf("Parent detached from shared memory\n");

        // Remove the shared memory segment.
        if (shmctl(shmid, IPC_RMID, NULL) == -1) {
            perror("shmctl");
            exit(EXIT_FAILURE);
        }
        printf("Shared memory segment removed\n");
    }

    return 0;
}
