/*
 * Milestone 3.2: guest mprotect() toggling a still-shared range between
 * PROT_READ and PROT_READ|WRITE after fork (cow-design.md §9.2: "mprotect
 * changing R/W bits on a Cow-backed range: does not need to materialize
 * -- a real host mprotect(PROT_WRITE) and a UFFDIO_WRITEPROTECT-registered
 * page compose correctly at the kernel level"). This is also the guest-
 * syscall side of the Milestone 0 spike question (cow-implementation-plan.md
 * Milestone 0 item 1: does UFFD-WP survive a real mprotect() call).
 *
 * The parent maps the range read-only *before* forking, so both parent
 * and child inherit it read-only and each independently has to mprotect
 * it back to read-write before writing -- exercising the toggle on both
 * sides of the fork, not just one.
 */
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (8UL * 1024 * 1024)
#define SEED_ORIG  0xABCD0000u
#define SEED_CHILD 0xEF010000u

int main(void) {
    unsigned char *buf = mmap(NULL, SIZE, PROT_READ | PROT_WRITE,
                               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    COW_CHECK(buf != MAP_FAILED, "mmap failed");
    cow_fill(buf, SIZE, SEED_ORIG);
    COW_CHECK(mprotect(buf, SIZE, PROT_READ) == 0, "parent mprotect(RO) failed");

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        if (!cow_verify(buf, SIZE, SEED_ORIG, "child-while-readonly")) _exit(1);

        COW_CHECK(mprotect(buf, SIZE, PROT_READ | PROT_WRITE) == 0, "child mprotect(RW) failed");
        cow_fill(buf, SIZE, SEED_CHILD);
        if (!cow_verify(buf, SIZE, SEED_CHILD, "child-after-write")) _exit(1);
        _exit(0);
    }

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    /* parent toggles its own copy back to RW and must still see the
     * original data -- the child's mprotect+write on its own copy must
     * not have disturbed the parent's (still read-only until now) range. */
    COW_CHECK(mprotect(buf, SIZE, PROT_READ | PROT_WRITE) == 0, "parent mprotect(RW) failed");
    COW_CHECK(cow_verify(buf, SIZE, SEED_ORIG, "parent-final"),
               "parent memory corrupted by child's mprotect/write");

    munmap(buf, SIZE);
    return 0;
}
