/*
 * Milestone 1: basic fork write path, parent diverges first
 * (cow-design.md §10 "parent-first write" -- symmetric with §9, but the
 * design explicitly calls out "there is no special 'parent owns the
 * original page' rule", so this needs its own test rather than assuming
 * §9's coverage is enough).
 *
 * Uses a pipe to force an observable ordering: parent writes and signals
 * before the child ever looks at the buffer, so the child's read is
 * guaranteed to happen after the parent's divergence. The child must
 * still see the ORIGINAL pattern (not the parent's new one), and after
 * the child later writes its own pattern and exits, the parent must
 * still see its own (already-diverged) data untouched by the child.
 */
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (16UL * 1024 * 1024)
#define SEED_ORIG   0x11111111u
#define SEED_PARENT 0x22222222u
#define SEED_CHILD  0x33333333u

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_ORIG);

    int pipefd[2];
    COW_CHECK(pipe(pipefd) == 0, "pipe failed");

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        close(pipefd[1]);
        char c;
        /* block until the parent has already written its own pattern */
        COW_CHECK(read(pipefd[0], &c, 1) == 1, "child: read from pipe failed");
        close(pipefd[0]);

        /* child must see the ORIGINAL data, not the parent's new write */
        if (!cow_verify(buf, SIZE, SEED_ORIG, "child-after-parent-write")) _exit(1);

        cow_fill(buf, SIZE, SEED_CHILD);
        if (!cow_verify(buf, SIZE, SEED_CHILD, "child-post-own-write")) _exit(1);
        _exit(0);
    }

    close(pipefd[0]);
    cow_fill(buf, SIZE, SEED_PARENT);
    COW_CHECK(cow_verify(buf, SIZE, SEED_PARENT, "parent-post-write"), "parent write failed");

    char c = 'x';
    COW_CHECK(write(pipefd[1], &c, 1) == 1, "parent: write to pipe failed");
    close(pipefd[1]);

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    /* parent's own (already-diverged) data must be unaffected by the child's write */
    COW_CHECK(cow_verify(buf, SIZE, SEED_PARENT, "parent-final"),
               "parent memory clobbered by child's later write");

    free(buf);
    return 0;
}
