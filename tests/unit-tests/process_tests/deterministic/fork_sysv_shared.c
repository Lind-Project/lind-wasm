#include <assert.h>
#include <limits.h>
#include <sys/ipc.h>
#include <sys/random.h>
#include <sys/shm.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    unsigned int random_key;
    assert(getrandom(&random_key, sizeof(random_key), 0) == sizeof(random_key));
    key_t key = (key_t)((random_key & INT_MAX) | 1);
    int shmid = shmget(key, 4096, IPC_CREAT | IPC_EXCL | 0600);
    assert(shmid >= 0);
    int *shared = shmat(shmid, NULL, 0);
    assert(shared != (void *)-1);
    *shared = 17;
    assert(shmctl(shmid, IPC_RMID, NULL) == 0);

    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        if (*shared != 17) {
            _exit(1);
        }
        *shared = 91;
        _exit(shmdt(shared) == 0 ? 0 : 2);
    }

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    assert(*shared == 91);

    pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        *shared = 103;
        _exit(0);
    }
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    assert(*shared == 103);
    struct shmid_ds info;
    assert(shmctl(shmid, IPC_STAT, &info) == 0);
    assert(info.shm_nattch == 1);
    assert(shmdt(shared) == 0);
    return 0;
}
