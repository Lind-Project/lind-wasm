/*
 * Milestone 3.2: guest mmap(MAP_FIXED) replacing HALF of a still-shared
 * range after fork (cow-design.md §9.2: "mmap(MAP_FIXED) replacing a
 * Cow-backed range: materialize first so the old backing's refcount is
 * decremented cleanly, then the existing overwrite logic installs
 * whatever new mapping the guest asked for").
 *
 * Checks both halves independently: the freshly remapped half must read
 * as zero, and the untouched half must still show the inherited data --
 * i.e. a MAP_FIXED overwrite of PART of a shared extent must not disturb
 * the rest of that extent for either side.
 */
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE  (8UL * 1024 * 1024)
#define HALF  (SIZE / 2)
#define SEED_ORIG 0xF00DFACEu

int main(void) {
    unsigned char *buf = mmap(NULL, SIZE, PROT_READ | PROT_WRITE,
                               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    COW_CHECK(buf != MAP_FAILED, "mmap failed");
    cow_fill(buf, SIZE, SEED_ORIG);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        unsigned char *second_half = buf + HALF;
        unsigned char *fresh = mmap(second_half, HALF, PROT_READ | PROT_WRITE,
                                     MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
        if (fresh == MAP_FAILED) _exit(1);

        if (!cow_all_zero(second_half, HALF, "child-remapped-half")) _exit(1);
        if (!cow_verify_range(buf, 0, HALF, SEED_ORIG, "child-untouched-half")) _exit(1);
        _exit(0);
    }

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    /* parent's full original range must be unaffected by the child's
     * partial MAP_FIXED overwrite of its own copy. */
    COW_CHECK(cow_verify(buf, SIZE, SEED_ORIG, "parent-after-child-remap"),
               "parent memory corrupted by child's MAP_FIXED overwrite");

    munmap(buf, SIZE);
    return 0;
}
