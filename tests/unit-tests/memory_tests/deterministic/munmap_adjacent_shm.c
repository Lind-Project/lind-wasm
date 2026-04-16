// Test: unaligned-length munmap must not clobber an adjacent shm page.
//
// Regression test for the bug fixed in PR #1075:
//   munmap_syscall rounded `len` up to a page multiple and issued
//   mmap(MAP_FIXED|PROT_NONE) over the whole rounded range, silently
//   replacing the first page of an adjacent shm segment.
//
// Layout (lind allocates top-down: shmat lands at the top of the first
// free gap, the subsequent anon mmap is placed immediately below it):
//
//     [ anon page ][ shm page ]
//         ^anon      ^shm = anon + PAGE
//
// Calling munmap(anon, PAGE+1) rounds the length up to 2*PAGE. The buggy
// code replaces both pages with PROT_NONE anonymous memory; reading the
// shm page then traps. The fixed code only PROT_NONEs the anon page
// because the adjacent page's vmmap entry is SharedMemory-backed.
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

    // Precondition: allocator placed anon immediately below shm.
    // If this ever changes, fail loudly rather than silently passing.
    assert(shm == anon + PAGE_SIZE &&
           "allocator layout changed: anon not adjacent-below shm");

    memset(anon, 0xCD, PAGE_SIZE);

    // Unmap one byte past a page. Length rounds up to 2*PAGE.
    // Buggy munmap would PROT_NONE the shm page too.
    int rc = munmap(anon, PAGE_SIZE + 1);
    assert(rc == 0 && "munmap failed");

    // The shm page must still be readable and hold its contents.
    for (int i = 0; i < PAGE_SIZE; i++) {
        if ((unsigned char)shm[i] != 0xAB) {
            printf("FAIL: shm[%d] = 0x%02x, expected 0xAB\n",
                   i, (unsigned char)shm[i]);
            shmdt(shm);
            shmctl(shmid, IPC_RMID, NULL);
            return 1;
        }
    }

    printf("PASS: adjacent shm page intact after unaligned munmap\n");
    shmdt(shm);
    shmctl(shmid, IPC_RMID, NULL);
    return 0;
}
