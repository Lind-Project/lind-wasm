/*
 * Milestone 1 / Milestone 3.4: fork() immediately followed by exec() in
 * the child (cow-design.md §14, cow-implementation-plan.md Milestone 3
 * item 3.4 "fork() immediately followed by exec() in the child releases
 * the parent's shared range's refcount correctly without the parent ever
 * seeing a change"). This is the headline workload the whole design is
 * optimizing for (cow-design.md §1, §26 case A).
 *
 * Self-execs via argv[0] (same pattern as
 * tests/unit-tests/process_tests/deterministic/test_exec_nofork.c) so no
 * separate helper binary/lindfs placement is needed.
 */
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (16UL * 1024 * 1024)
#define SEED_PARENT 0x900DF00Du

int main(int argc, char *argv[]) {
    if (argc > 1 && strcmp(argv[1], "--child") == 0) {
        /* post-exec re-entry: the old address space (and the buffer
         * below) is gone; nothing to check here, exec() replaced it. */
        return 0;
    }

    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_PARENT);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        char *child_argv[] = {argv[0], "--child", NULL};
        execv(argv[0], child_argv);
        /* only reached if execv failed */
        _exit(127);
    }

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child exec failed");

    /* parent's memory must be intact: the child's exec()-driven address
     * space replacement must not have disturbed the parent's shared
     * backing for the range it inherited but never touched. */
    COW_CHECK(cow_verify(buf, SIZE, SEED_PARENT, "parent-post-fork-exec"),
               "parent memory changed after child's fork+exec");

    free(buf);
    return 0;
}
