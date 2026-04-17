// Test: unaligned-length munmap must not clobber a nearby shm page.
//
// Regression test for the bug fixed in PR #1075:
//   munmap_syscall rounded `len` up to a page multiple and issued a single
//   mmap(MAP_FIXED|PROT_NONE) over the whole rounded range. When that
//   range crossed into a SharedMemory-backed vmmap entry, the host-level
//   PROT_NONE silently clobbered the shm page.
//
// Layout strategy (no MAP_FIXED, no shmat hint):
//   lind's allocator places NULL-addr shmat / mmap at the top of a free
//   gap, and empirically leaves a 1-page stride between consecutive
//   NULL-addr allocations. So:
//
//     shmat(NULL) -> shm at top page T
//     mmap (NULL) -> anon at T - 2*PAGE
//     page at   T - PAGE  -> unmapped gap
//
//     [ anon @ T-2P ][ gap @ T-P ][ shm @ T ]
//
// munmap(anon, 2*PAGE+1) rounds up to 3*PAGE and targets [T-2P .. T+P),
// covering {anon, gap, shm}. The shm entry's backing is SharedMemory, so:
//
//   Buggy path: one mmap(PROT_NONE, MAP_FIXED) over the full 3*PAGE range
//               clobbers shm's host memory -> reading shm[0] faults.
//   Fixed path: the overlap loop filters out the SharedMemory entry;
//               only the anon page gets PROT_NONE'd. shm stays readable.
//
// The stride is asserted explicitly — if allocator behavior ever changes
// and anon doesn't land at T - 2*PAGE, the test fails loudly instead of
// silently passing.
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

    // Precondition on allocator layout: anon must land exactly 2 pages
    // below shm (one page of unmapped gap between them). If this ever
    // changes, fail loudly instead of passing by accident.
    assert(shm == anon + 2 * PAGE_SIZE &&
           "allocator layout changed: anon is not at shm - 2*PAGE");

    // Unaligned munmap starting at anon, ending one byte past the gap:
    //   len = 2*PAGE+1  ->  rounded length = 3*PAGE
    //   rounded range   =  [anon, anon + 3*PAGE) = [anon, shm + PAGE)
    // Buggy runtime PROT_NONEs the whole range (clobbering shm).
    // Fixed runtime skips the SharedMemory-backed page.
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
