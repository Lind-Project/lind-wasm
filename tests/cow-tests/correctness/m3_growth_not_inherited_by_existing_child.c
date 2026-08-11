/*
 * Milestone 3 / cow-design.md §24 Invariant 6 (broadened): "A cage cannot
 * inherit a sibling's or ex-parent's post-fork vmmap mutations of any
 * kind -- not just growth." This test exercises the mmap-driven case
 * (the Lind analog of the original design's "memory.grow after fork must
 * not leak to an existing child", cow-design.md §13).
 *
 * Sequence:
 *   1. Parent mmaps R1, fills pattern A, forks child1.
 *   2. child1 blocks on a pipe (so it can't observe anything the parent
 *      does next).
 *   3. Parent mmaps a NEW region R2 (created strictly after child1
 *      already exists) and fills pattern B, then wakes child1.
 *   4. child1 must still see R1 == A. It also does its own independent
 *      mmap for a same-sized fresh region and confirms that comes back
 *      zeroed -- not aliased to the parent's R2 content -- ruling out an
 *      address-collision-shaped version of "growth leaking into an
 *      existing child".
 *   5. Parent reaps child1, then forks child2 (created strictly after R2
 *      existed). child2 must inherit BOTH R1 == A and R2 == B correctly.
 *   6. Parent verifies its own R1 and R2 are untouched by either child.
 */
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (4UL * 1024 * 1024)
#define SEED_A 0x0A0A0A0Au
#define SEED_B 0x0B0B0B0Bu

int main(void) {
    unsigned char *r1 = mmap(NULL, SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    COW_CHECK(r1 != MAP_FAILED, "mmap r1 failed");
    cow_fill(r1, SIZE, SEED_A);

    int pipefd[2];
    COW_CHECK(pipe(pipefd) == 0, "pipe failed");

    pid_t pid1 = fork();
    COW_CHECK(pid1 >= 0, "fork child1 failed");

    if (pid1 == 0) {
        close(pipefd[1]);
        char c;
        COW_CHECK(read(pipefd[0], &c, 1) == 1, "child1: read pipe failed");
        close(pipefd[0]);

        if (!cow_verify(r1, SIZE, SEED_A, "child1-r1")) _exit(1);

        unsigned char *probe = mmap(NULL, SIZE, PROT_READ | PROT_WRITE,
                                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (probe == MAP_FAILED) _exit(1);
        if (!cow_all_zero(probe, SIZE, "child1-probe")) _exit(1);
        _exit(0);
    }

    close(pipefd[0]);
    unsigned char *r2 = mmap(NULL, SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    COW_CHECK(r2 != MAP_FAILED, "mmap r2 failed");
    cow_fill(r2, SIZE, SEED_B);

    char c = 'x';
    COW_CHECK(write(pipefd[1], &c, 1) == 1, "parent: write pipe failed");
    close(pipefd[1]);

    int status;
    COW_CHECK(waitpid(pid1, &status, 0) == pid1, "waitpid child1 failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child1 failed");

    pid_t pid2 = fork();
    COW_CHECK(pid2 >= 0, "fork child2 failed");
    if (pid2 == 0) {
        if (!cow_verify(r1, SIZE, SEED_A, "child2-r1")) _exit(1);
        if (!cow_verify(r2, SIZE, SEED_B, "child2-r2")) _exit(1);
        _exit(0);
    }
    COW_CHECK(waitpid(pid2, &status, 0) == pid2, "waitpid child2 failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child2 failed");

    COW_CHECK(cow_verify(r1, SIZE, SEED_A, "parent-final-r1"), "parent r1 corrupted");
    COW_CHECK(cow_verify(r2, SIZE, SEED_B, "parent-final-r2"), "parent r2 corrupted");

    munmap(r1, SIZE);
    munmap(r2, SIZE);
    return 0;
}
