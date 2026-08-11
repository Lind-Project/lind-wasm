/*
 * Milestone 3.2 (cow-implementation-plan.md): vmmap-mutating-syscall
 * integration -- guest munmap() of a still-shared range after fork, before
 * either side has written to it (cow-design.md §9.2, "materialize_unique
 * guard... Guest munmap of a Cow-backed range").
 *
 * Uses an explicit mmap (rather than malloc) so the address and the
 * mapping's lifecycle are under direct control.
 */
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (8UL * 1024 * 1024)
#define SEED_ORIG 0x600D0000u

int main(void) {
    unsigned char *buf = mmap(NULL, SIZE, PROT_READ | PROT_WRITE,
                               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    COW_CHECK(buf != MAP_FAILED, "mmap failed");
    cow_fill(buf, SIZE, SEED_ORIG);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        /* child: munmap its copy of the still-shared range before ever
         * writing to it via a direct store, then remap fresh memory at
         * the same address and confirm it reads as zero (not stale/
         * leftover shared content, and not a crash from a dangling COW
         * registration on the freed range). */
        COW_CHECK(munmap(buf, SIZE) == 0, "child munmap failed");

        unsigned char *fresh = mmap(buf, SIZE, PROT_READ | PROT_WRITE,
                                     MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
        if (fresh == MAP_FAILED) _exit(1);
        if (!cow_all_zero(fresh, SIZE, "child-fresh-remap")) _exit(1);
        _exit(0);
    }

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    /* the child's munmap()+remap of ITS OWN address space must not have
     * released/corrupted the parent's still-live shared backing. */
    COW_CHECK(cow_verify(buf, SIZE, SEED_ORIG, "parent-after-child-munmap"),
               "parent memory corrupted by child's munmap");

    munmap(buf, SIZE);
    return 0;
}
