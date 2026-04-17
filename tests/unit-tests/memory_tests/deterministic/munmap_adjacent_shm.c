// Regression test for PR #1075: munmap with unaligned length must not clobber adjacent shm pages.
//
// Layout: shmat(NULL) -> shm @ T, mmap(NULL) -> anon @ T-2P, gap @ T-P.
// munmap(anon, 2*PAGE+1) rounds to 3*PAGE, covering [T-2P, T+P).
// Buggy path: single PROT_NONE over full range wipes shm. Fixed path: shm skipped.
// Stride is asserted — if allocator changes, test fails loudly rather than passing silently.
#include <sys/ipc.h>
#include <sys/shm.h>
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>
#include <stdint.h>

#define PAGE_SIZE 4096

int main(void) {
    key_t key = 4242;
    int shmid = shmget(key, PAGE_SIZE, 0666 | IPC_CREAT);
    assert(shmid != -1 && "shmget failed");

    char *shm = (char *)shmat(shmid, NULL, 0);
    assert(shm != (char *)-1 && "shmat failed");
    assert(((uintptr_t)shm % PAGE_SIZE) == 0 && "shm not page-aligned");
    memset(shm, 0xAB, PAGE_SIZE);

    char *anon = (char *)mmap(NULL, PAGE_SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(anon != MAP_FAILED && "anon mmap failed");
    memset(anon, 0xCD, PAGE_SIZE);

    // Allocator must place anon at shm - 2*PAGE; fail loudly if layout changes.
    assert(shm == anon + 2 * PAGE_SIZE &&
           "allocator layout changed: anon is not at shm - 2*PAGE");

    // munmap(anon, 2*PAGE+1) rounds to 3*PAGE -> [anon, shm+PAGE). Shm must survive.
    int rc = munmap(anon, 2 * PAGE_SIZE + 1);
    assert(rc == 0 && "trigger munmap failed");

    for (int i = 0; i < PAGE_SIZE; i++) {
        if ((unsigned char)shm[i] != 0xAB) {
            printf("FAIL: shm[%d] = 0x%02x, expected 0xAB\n",
                   i, (unsigned char)shm[i]);
            shmdt(shm);
            shmctl(shmid, IPC_RMID, NULL);
            return 1;
        }
    }

    printf("PASS: shm page intact after unaligned munmap of nearby anon\n");
    shmdt(shm);
    shmctl(shmid, IPC_RMID, NULL);
    return 0;
}
