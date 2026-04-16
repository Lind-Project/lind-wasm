// Test: unaligned-length munmap must not clobber an adjacent shm page.
//
// Regression test for the bug fixed in PR #1075:
//   munmap_syscall rounded `len` up to a page multiple and issued a single
//   mmap(MAP_FIXED|PROT_NONE) over the whole rounded range. When that
//   range crossed into an adjacent SharedMemory-backed vmmap entry, the
//   host-level PROT_NONE silently clobbered the shm page.
//
// Forcing adjacency without MAP_FIXED:
//   lind's allocator places allocations at the TOP of a gap, so a plain
//   mmap after shmat doesn't land byte-adjacent to shm (there's typically
//   a startup-time entry one page below shm). MAP_FIXED would bypass the
//   allocator but silently overwrite whatever that entry is via
//   add_entry_with_overwrite. Instead we carve a 1-page hole inside an
//   anon region we own and steer shmat into it:
//
//     1. mmap(NULL, 3*PAGE) → [base .. base+3P) anon
//     2. munmap(base+PAGE, PAGE) → splits into two anon entries with a
//        1-page gap at [base+PAGE .. base+2P)
//     3. shmat(shmid, base+PAGE, 0) → find_map_space_with_hint iterates
//        gaps from the hint upward and returns the top of the first
//        fitting gap; the 1-page hole is exactly 1 page, so shm lands
//        there byte-exact
//
// Layout after setup:
//
//     [ anon @ base ][ shm @ base+PAGE ][ anon @ base+2*PAGE ]
//
// munmap(base, PAGE+1) rounds to 2*PAGE and targets [base .. base+2P),
// covering {anon, shm}. Buggy: single mmap(PROT_NONE) clobbers shm's
// host memory. Fixed: overlap loop filters out SharedMemory entries and
// only PROT_NONEs the anon page.
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

    char *base = (char *)mmap(NULL, 3 * PAGE_SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(base != MAP_FAILED && "anon mmap failed");
    assert(((uintptr_t)base % PAGE_SIZE) == 0 && "base not page-aligned");

    // Carve a 1-page hole in the middle. The buggy munmap path is what
    // PR #1075 touches — but a clean unmap of a fully-contained anon
    // page does not trip the bug, so this step is safe on both builds.
    int rc = munmap(base + PAGE_SIZE, PAGE_SIZE);
    assert(rc == 0 && "middle munmap failed");

    // Steer shmat into the 1-page hole via the hint argument.
    char *shm = (char *)shmat(shmid, base + PAGE_SIZE, 0);
    assert(shm != (char *)-1 && "shmat failed");
    assert(shm == base + PAGE_SIZE &&
           "shmat did not land in carved hole — allocator may have "
           "found a larger gap above the hint");

    memset(shm, 0xAB, PAGE_SIZE);

    // Trigger: unaligned munmap at `base` whose rounded length (2*PAGE)
    // reaches into the shm page. The fixed runtime must leave shm's host
    // memory R/W because the overlapping entry is SharedMemory-backed.
    rc = munmap(base, PAGE_SIZE + 1);
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

    printf("PASS: adjacent shm page intact after unaligned munmap\n");
    shmdt(shm);
    shmctl(shmid, IPC_RMID, NULL);
    return 0;
}
