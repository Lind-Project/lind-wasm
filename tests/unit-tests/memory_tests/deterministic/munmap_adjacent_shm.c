// Test: unaligned-length munmap must not clobber an adjacent shm page.
//
// Regression test for the bug fixed in PR #1075:
//   munmap_syscall rounded `len` up to a page multiple and issued
//   mmap(MAP_FIXED|PROT_NONE) over the whole rounded range, silently
//   replacing the first page of an adjacent shm segment.
//
// Layout (relies on lind's top-down gap allocation: shmat is allocated
// first and placed at the top of the first free gap; the subsequent mmap
// is then placed immediately below it):
//
//     [ anon page ][ shm page ]
//         ^anon      ^shm = anon + PAGE
//
// An explicit assert on adjacency makes the precondition fail loudly
// (not silently pass) if allocator behavior ever diverges.
//
// Calling munmap(anon, PAGE+1) rounds the length up to 2*PAGE. The buggy
// code replaces both pages with PROT_NONE anonymous memory; reading
// the shm page then traps. The fixed code only PROT_NONEs the anon page
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

    printf("DIAG: shm  = %p\n", (void *)shm);

    char *probe[8];
    for (int i = 0; i < 8; i++) {
        probe[i] = (char *)mmap(NULL, PAGE_SIZE, PROT_READ | PROT_WRITE,
                                MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        assert(probe[i] != MAP_FAILED && "probe mmap failed");
        printf("DIAG: probe[%d] = %p\n", i, (void *)probe[i]);
    }

    printf("FAIL: diagnostic probe run — review layout above.\n");
    for (int i = 0; i < 8; i++) munmap(probe[i], PAGE_SIZE);
    shmdt(shm);
    shmctl(shmid, IPC_RMID, NULL);
    return 1;

    anon[0] = 0x11;
    anon[PAGE_SIZE - 1] = 0x22;

    int rc = munmap(anon, PAGE_SIZE + 1);
    assert(rc == 0 && "munmap with unaligned length failed");

    for (size_t i = 0; i < PAGE_SIZE; i++) {
        assert(shm[i] == (char)0xAB && "shm page clobbered by adjacent munmap");
    }

    assert(shmdt(shm) == 0 && "shmdt failed");
    assert(shmctl(shmid, IPC_RMID, NULL) == 0 && "shmctl IPC_RMID failed");

    printf("munmap_adjacent_shm test: PASS\n");
    return 0;
}
