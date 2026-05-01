#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

/*
 * Test that we can fork up to the cage ID limit (MAX_CAGEID - 1 usable
 * cages) and that the next fork fails gracefully.
 *
 * Cage IDs are monotonic and never recycled, so we fork in a tight
 * loop with children exiting immediately, waitpid after each, and
 * count successes until fork returns -1.
 */

int main(void)
{
    int count = 0;

    for (;;) {
        pid_t pid = fork();

        if (pid < 0) {
            /* Expected: cage ID space exhausted */
            break;
        }

        if (pid == 0) {
            /* child: exit immediately */
            _exit(0);
        }

        /* parent */
        int status;
        pid_t w = waitpid(pid, &status, 0);
        assert(w >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
        count++;
    }

    /*
     * MAX_CAGEID = 2048, CAGE_MAP indices 0..2047.
     * Cage 0 is unused, cage 1 is the initial cage.
     * alloc_cage_id returns 2..2047 = 2046 cages.
     */
    printf("fork_max_cages: forked %d children successfully\n", count);
    assert(count == 2046);

    printf("fork_max_cages: all tests passed\n");
    return 0;
}
