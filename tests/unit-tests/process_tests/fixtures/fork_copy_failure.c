#include <assert.h>
#include <errno.h>
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

    errno = 0;
    assert(fork() == -1);
    assert(errno == ENOMEM);

    int status;
    errno = 0;
    assert(waitpid(-1, &status, WNOHANG) == -1);
    assert(errno == ECHILD);
    assert(*shared == 17);
    struct shmid_ds info;
    assert(shmctl(shmid, IPC_STAT, &info) == 0);
    assert(info.shm_nattch == 1);
    assert(shmdt(shared) == 0);
    return 0;
}
