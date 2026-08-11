/*
 * Milestone 3.3 (cow-implementation-plan.md): N-way concurrent writers
 * racing on the same original backing (cow-design.md §11: "A, B, C all
 * reference P ref=3... The first writer copies once. The second writer
 * observes the updated reference count/state and copies only if its
 * current backing is still shared").
 *
 * Three children are forked back-to-back with no synchronization barrier
 * between them, so the runtime is free to schedule/execute them
 * overlapping in time (each is a fully isolated cage/address space --
 * this is not a same-cage data race, it specifically targets the shared
 * BACKING object's refcount/materialize bookkeeping under N-way
 * divergence). Each child overwrites the ENTIRE inherited buffer with a
 * distinct pattern; the parent's own view must remain untouched no
 * matter the interleaving.
 */
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE (8UL * 1024 * 1024)
#define SEED_ORIG 0x50000000u
#define NUM_CHILDREN 3

static int run_child(unsigned char *buf, uint32_t own_seed) {
    if (!cow_verify(buf, SIZE, SEED_ORIG, "child-pre-write")) return 1;
    cow_fill(buf, SIZE, own_seed);
    if (!cow_verify(buf, SIZE, own_seed, "child-post-write")) return 1;
    return 0;
}

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_ORIG);

    pid_t children[NUM_CHILDREN];
    for (int i = 0; i < NUM_CHILDREN; i++) {
        pid_t pid = fork();
        COW_CHECK(pid >= 0, "fork failed");
        if (pid == 0) {
            _exit(run_child(buf, SEED_ORIG + 0x1000u * (uint32_t)(i + 1)));
        }
        children[i] = pid;
        /* deliberately no wait here -- forked back-to-back so the
         * runtime can schedule them concurrently. */
    }

    int all_ok = 1;
    for (int i = 0; i < NUM_CHILDREN; i++) {
        int status;
        if (waitpid(children[i], &status, 0) != children[i]) all_ok = 0;
        if (!(WIFEXITED(status) && WEXITSTATUS(status) == 0)) all_ok = 0;
    }
    COW_CHECK(all_ok, "one or more children failed");

    COW_CHECK(cow_verify(buf, SIZE, SEED_ORIG, "parent-final"),
               "parent memory corrupted by concurrent sibling writers");

    free(buf);
    return 0;
}
