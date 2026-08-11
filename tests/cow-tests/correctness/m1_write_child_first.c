/*
 * Milestone 1: basic fork write path, child diverges first
 * (cow-design.md §9 "write path: COW split", the "child writes first"
 * case from cow-implementation-plan.md Milestone 1 step 1.6).
 *
 * Scenario: parent fills a buffer, forks; child overwrites the whole
 * inherited range with a different pattern and verifies its own write
 * stuck. Parent (after reaping the child) must still see its original
 * data -- the core "first divergent write preserves old readers"
 * invariant (cow-design.md §24 Invariant 3).
 */
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (16UL * 1024 * 1024)
#define SEED_PARENT 0xA5A5A5u
#define SEED_CHILD  0x5A5A5Au

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_PARENT);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        if (!cow_verify(buf, SIZE, SEED_PARENT, "child-pre-write")) _exit(1);
        cow_fill(buf, SIZE, SEED_CHILD);
        if (!cow_verify(buf, SIZE, SEED_CHILD, "child-post-write")) _exit(1);
        _exit(0);
    }

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    COW_CHECK(cow_verify(buf, SIZE, SEED_PARENT, "parent-post-child-write"),
               "parent memory clobbered by child's write");

    free(buf);
    return 0;
}
