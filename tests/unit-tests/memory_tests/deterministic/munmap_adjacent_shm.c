// Test: unaligned-length munmap must not clobber a nearby shm page.
//
// Regression test for the bug fixed in PR #1075:
//   munmap_syscall rounded `len` up to a page multiple and issued a single
//   mmap(MAP_FIXED|PROT_NONE) over the whole rounded range. When that range
//   crossed into a SharedMemory-backed vmmap entry, the host-level PROT_NONE
//   silently clobbered the shm page.
//
// Layout strategy:
//   1. shmat(NULL) -> shm lands at some address T (allocator decides)
//   2. mmap(T - 2*PAGE, ..., MAP_FIXED) -> anon forced to exactly T - 2*PAGE
//   3. Gap page at T - PAGE is unmapped
//
//     [ anon @ T-2P ][ gap @ T-P ][ shm @ T ]
//
//   munmap(anon, 2*PAGE+1) rounds up to 3*PAGE and targets [T-2P .. T+P),
//   covering {anon, gap, shm}.
//
//   Buggy path: one mmap(PROT_NONE, MAP_FIXED) over the full 3*PAGE range
//               clobbers shm's host memory -> reading shm[0] faults.
//   Fixed path: the overlap loop filters out the SharedMemory entry;
//               only the anon/gap pages get PROT_NONE'd. shm stays readable.
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

    // Force anon exactly 2 pages below shm using MAP_FIXED.
    // We control anon's address; we don't need the allocator to cooperate.
    char *anon_target = shm - 2 * PAGE_SIZE;
    char *anon = (char *)mmap(anon_target, PAGE_SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);

    fprintf(stderr, "  DIAG: shm       @ %p\n", (void *)shm);
    fprintf(stderr, "  DIAG: anon_target= %p\n", (void *)anon_target);
    fprintf(stderr, "  DIAG: anon actual= %p\n", (void *)anon);
    fprintf(stderr, "  DIAG: gap        @ %p\n", (void *)(shm - PAGE_SIZE));
    fprintf(stderr, "  DIAG: munmap range [%p, %p) — rounded len = 3 pages\n",
            (void *)anon, (void *)(anon + 3 * PAGE_SIZE));

    if (anon == MAP_FAILED) {
        fprintf(stderr, "FAIL: MAP_FIXED mmap for anon failed\n");
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    if (anon != anon_target) {
        fprintf(stderr,
                "FAIL: MAP_FIXED returned %p instead of %p — "
                "kernel rejected the fixed placement\n",
                (void *)anon, (void *)anon_target);
        munmap(anon, PAGE_SIZE);
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }

    memset(anon, 0xCD, PAGE_SIZE);

    // munmap(anon, 2*PAGE+1):
    //   rounded length = 3*PAGE
    //   rounded range  = [anon, anon+3*PAGE) = [T-2P, T+P)
    // Buggy runtime: PROT_NONEs the full range including shm at T.
    // Fixed runtime: skips the SharedMemory-backed page at T.
    int rc = munmap(anon, 2 * PAGE_SIZE + 1);
    assert(rc == 0 && "trigger munmap failed");

    fprintf(stderr, "  DIAG: munmap returned 0, checking shm integrity\n");

    for (int i = 0; i < PAGE_SIZE; i++) {
        if ((unsigned char)shm[i] != 0xAB) {
            fprintf(stderr, "FAIL: shm[%d] = 0x%02x, expected 0xAB\n",
                    i, (unsigned char)shm[i]);
            shmdt(shm);
            shmctl(shmid, IPC_RMID, NULL);
            return 1;
        }
    }

    fprintf(stderr, "  DIAG: all %d shm bytes intact\n", PAGE_SIZE);
    printf("PASS: shm page intact after unaligned munmap of nearby anon\n");
    shmdt(shm);
    shmctl(shmid, IPC_RMID, NULL);
    return 0;
}
