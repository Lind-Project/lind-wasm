// Regression test: munmap with unaligned length must not destroy adjacent shm segment.
// Layout: [anon 2P][shm 1P], munmap(anon, 2P+1) rounds to 3P covering shm.
#include <sys/ipc.h>
#include <sys/shm.h>
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <errno.h>

#define PAGE_SIZE 4096

int main(void) {
    // Create and attach shm
    int shmid = shmget(4242, PAGE_SIZE, 0666 | IPC_CREAT);
    if (shmid == -1) {
        perror("shmget");
        return 1;
    }

    char *shm = (char *)shmat(shmid, NULL, 0);
    if (shm == (char *)-1) {
        perror("shmat");
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    memset(shm, 0xAB, PAGE_SIZE);

    // Place anon immediately before shm using MAP_FIXED
    char *anon = (char *)mmap(shm - 2 * PAGE_SIZE, 2 * PAGE_SIZE,
                              PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (anon == MAP_FAILED) {
        printf("SKIP: MAP_FIXED failed (errno=%d)\n", errno);
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 77;
    }
    memset(anon, 0xCD, 2 * PAGE_SIZE);

    // munmap with unaligned length - rounds up to cover shm
    if (munmap(anon, 2 * PAGE_SIZE + 1) != 0) {
        perror("munmap");
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }

    // Verify shm segment still exists
    struct shmid_ds stat;
    if (shmctl(shmid, IPC_STAT, &stat) == -1) {
        printf("FAIL: shm segment destroyed\n");
        return 1;
    }

    // Re-attach and verify data
    char *shm2 = (char *)shmat(shmid, NULL, 0);
    if (shm2 == (char *)-1) {
        perror("shmat re-attach");
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }

    for (int i = 0; i < PAGE_SIZE; i++) {
        if ((unsigned char)shm2[i] != 0xAB) {
            printf("FAIL: shm[%d]=0x%02x, expected 0xAB\n", i, (unsigned char)shm2[i]);
            shmdt(shm2);
            shmctl(shmid, IPC_RMID, NULL);
            return 1;
        }
    }

    printf("PASS: shm intact after unaligned munmap\n");

    shmdt(shm2);
    shmctl(shmid, IPC_RMID, NULL);

    return 0;
}
