// Test: unaligned-length munmap must not clobber an adjacent shm page.
//
// Regression test for the bug fixed in PR #1075:
//   munmap_syscall rounded `len` up to a page multiple and issued
//   mmap(MAP_FIXED|PROT_NONE) over the whole rounded range, silently
//   replacing the first page of an adjacent shm segment.
//
// Layout forced via MAP_FIXED (so the repro does not rely on allocator
// placement heuristics):
//
//     [ anon page ][ shm page ]
//         ^addr=A    ^addr=A+PAGE
//
// Calling munmap(A, PAGE+1) rounds the length up to 2*PAGE. The buggy
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
    int shmid = shmget(IPC_PRIVATE, PAGE_SIZE, 0666 | IPC_CREAT);
    assert(shmid != -1 && "shmget failed");

    char *shm = (char *)shmat(shmid, NULL, 0);
    assert(shm != (char *)-1 && "shmat failed");
    assert(((uintptr_t)shm % PAGE_SIZE) == 0 && "shm not page-aligned");

    memset(shm, 0xAB, PAGE_SIZE);

    char *anon_hint = shm - PAGE_SIZE;
    char *anon = (char *)mmap(anon_hint, PAGE_SIZE, PROT_READ | PROT_WRITE,
                              MAP_FIXED | MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(anon != MAP_FAILED && "fixed mmap before shm failed");
    assert(anon == anon_hint && "MAP_FIXED did not honor requested address");

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
