/*
 * Milestone 2 (narrow pass): diamond/sibling fork tree, A -> B and A -> C
 * from the SAME unmodified parent state (cow-design.md §12: "This
 * supports arbitrary fork trees... with page versions shared wherever
 * possible", and cow-implementation-plan.md Milestone 2 step 2.2:
 * "verify sibling forks of the same parent correctly share/independently
 * diverge without cross-contaminating each other's refcount/backing
 * state").
 *
 * Sequential (B fully completes and is reaped before C is forked) so
 * this test isolates the "two independent children share the same
 * original backing correctly" property without also depending on
 * Milestone 3's true-concurrency handling (see m3_concurrent_sibling_writers.c
 * for the racing variant).
 */
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE   (16UL * 1024 * 1024)
#define HALF   (SIZE / 2)

#define SEED_A 0x11110000u
#define SEED_B 0x22220000u
#define SEED_C 0x33330000u

static int run_child(unsigned char *buf, size_t range_start, uint32_t own_seed, size_t other_start) {
    if (!cow_verify(buf, SIZE, SEED_A, "child: inherited")) return 1;

    cow_fill_range(buf, range_start, HALF, own_seed);
    if (!cow_verify_range(buf, range_start, HALF, own_seed, "child: own write")) return 1;
    if (!cow_verify_range(buf, other_start, HALF, SEED_A, "child: other half still original")) return 1;
    return 0;
}

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_A);

    /* --- B diverges the first half --- */
    pid_t pid_b = fork();
    COW_CHECK(pid_b >= 0, "fork A->B failed");
    if (pid_b == 0) {
        _exit(run_child(buf, 0, SEED_B, HALF));
    }
    int status;
    COW_CHECK(waitpid(pid_b, &status, 0) == pid_b, "waitpid B failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "B failed");

    /* A must still be pristine after B exits */
    COW_CHECK(cow_verify(buf, SIZE, SEED_A, "A-after-B"), "A corrupted by B");

    /* --- C diverges the second half, forked from the SAME original A --- */
    pid_t pid_c = fork();
    COW_CHECK(pid_c >= 0, "fork A->C failed");
    if (pid_c == 0) {
        _exit(run_child(buf, HALF, SEED_C, 0));
    }
    COW_CHECK(waitpid(pid_c, &status, 0) == pid_c, "waitpid C failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "C failed");

    /* A must still be fully pristine -- neither sibling's divergence,
     * nor the fact that both were forked from the same original range,
     * may have leaked into A or into each other (checked inside run_child). */
    COW_CHECK(cow_verify(buf, SIZE, SEED_A, "A-final"), "A corrupted by B and/or C");

    free(buf);
    return 0;
}
