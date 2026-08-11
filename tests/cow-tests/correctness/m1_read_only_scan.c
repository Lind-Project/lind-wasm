/*
 * Milestone 1 (cow-implementation-plan.md): basic fork, single-threaded,
 * no intervening memory syscalls.
 *
 * Scenario: parent fills a large private buffer, forks, and the child
 * only *reads* the inherited range (design doc cow-design.md §8 "read
 * path" -- the property that makes shared-backing COW win over the
 * UFFD_MISSING alternative, §28). Verifies: child sees byte-identical
 * inherited data; parent's own copy is unaffected by the child's read.
 */
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include "../common/cow_test.h"

#define SIZE (16UL * 1024 * 1024)
#define SEED_A 0xC0FFEEu

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_A);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        /* child: read-only scan of the entire inherited range */
        if (!cow_verify(buf, SIZE, SEED_A, "child")) {
            _exit(1);
        }
        _exit(0);
    }

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child scan failed");

    /* parent's own memory must be untouched by the child's read */
    COW_CHECK(cow_verify(buf, SIZE, SEED_A, "parent"), "parent memory changed after child read");

    free(buf);
    return 0;
}
