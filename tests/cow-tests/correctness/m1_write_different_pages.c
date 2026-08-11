/*
 * Milestone 1: parent and child diverge disjoint sub-ranges of the same
 * inherited buffer ("both write different pages" from
 * cow-implementation-plan.md Milestone 1 step 1.6). Exercises that a COW
 * split on one sub-range must not disturb sibling sub-ranges still
 * shared/inherited from the same original backing.
 */
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE       (16UL * 1024 * 1024)
#define QUARTER    (SIZE / 4)
#define SEED_ORIG  0xDEAD0000u
#define SEED_CHILD 0xBEEF0000u

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_ORIG);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        /* child dirties the SECOND quarter only, leaves the rest inherited */
        cow_fill_range(buf, QUARTER, QUARTER, SEED_CHILD);

        if (!cow_verify_range(buf, 0, QUARTER, SEED_ORIG, "child-q0")) _exit(1);
        if (!cow_verify_range(buf, QUARTER, QUARTER, SEED_CHILD, "child-q1")) _exit(1);
        if (!cow_verify_range(buf, 2 * QUARTER, QUARTER, SEED_ORIG, "child-q2")) _exit(1);
        if (!cow_verify_range(buf, 3 * QUARTER, QUARTER, SEED_ORIG, "child-q3")) _exit(1);
        _exit(0);
    }

    /* parent dirties the THIRD quarter concurrently with the child's write above */
    cow_fill_range(buf, 2 * QUARTER, QUARTER, SEED_CHILD + 1);

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    /* parent must see: its own q2 write, and original data everywhere else
     * (the child's q1 write must not have leaked into the parent). */
    COW_CHECK(cow_verify_range(buf, 0, QUARTER, SEED_ORIG, "parent-q0"), "parent q0 corrupted");
    COW_CHECK(cow_verify_range(buf, QUARTER, QUARTER, SEED_ORIG, "parent-q1"),
               "parent q1 leaked child's write");
    COW_CHECK(cow_verify_range(buf, 2 * QUARTER, QUARTER, SEED_CHILD + 1, "parent-q2"),
               "parent q2 (its own write) corrupted");
    COW_CHECK(cow_verify_range(buf, 3 * QUARTER, QUARTER, SEED_ORIG, "parent-q3"),
               "parent q3 corrupted");

    free(buf);
    return 0;
}
