#include <assert.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/ipc.h>
#include <sys/shm.h>
#include <unistd.h>
#include <sys/wait.h>

#define SHM_SIZE 4096

int main() {
    key_t key = 1234;
    int shmid;
    int *flag;

    // Try to remove any existing segment first (ignore errors)
    int old_shmid = shmget(key, SHM_SIZE, 0666);
    if (old_shmid >= 0) {
        shmctl(old_shmid, IPC_RMID, NULL);
    }

    // Create the shared memory segment
    shmid = shmget(key, SHM_SIZE, IPC_CREAT | IPC_EXCL | 0666);
    assert(shmid >= 0);

    // Parent: attach to shared memory and initialize flag to 0
    void *shmaddr = shmat(shmid, NULL, 0);
    assert(shmaddr != (void *)-1);
    flag = (int *)shmaddr;
    *flag = 0;

    // Fork
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        // Child: attach to shared memory (gets its own pointer to same segment)
        void *child_shmaddr = shmat(shmid, NULL, 0);
        assert(child_shmaddr != (void *)-1);
        
        // Write flag = 777
        int *child_flag = (int *)child_shmaddr;
        *child_flag = 777;

        // Note: child doesn't need to detach before exit
        _exit(0);
    } else {
        // Parent: wait for child
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);

        // Assert that the shared memory was updated by child
        assert(*flag == 777);

        // Remove the shared memory segment (detach happens automatically on process exit)
        assert(shmctl(shmid, IPC_RMID, NULL) == 0);
    }

    return 0;
}
