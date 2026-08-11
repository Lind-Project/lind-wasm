/*
 * Milestone 2 (narrow / architecture-validation pass, per
 * cow-implementation-plan.md): recursive fork chain A -> B -> C, each
 * generation diverging a disjoint sub-range via direct writes only (no
 * intervening mmap/munmap/mprotect/brk -- that variant is Milestone 3.5's
 * "realistic pass").
 *
 * This directly checks cow-design.md §12's central claim: "every page,
 * including Q2, still has a Lind page-store identity" -- i.e. C (the
 * grandchild) must see B's diverged range with B's data, AND the
 * still-untouched majority of the buffer with the ORIGINAL A data, even
 * though C never interacted with A directly.
 *
 * Chain is kept strictly sequential (B waits for C before B exits, A
 * waits for B) to avoid needing orphan-reparenting semantics.
 */
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE   (16UL * 1024 * 1024)
#define TENTH  (SIZE / 10)

#define SEED_A 0xA0000000u
#define SEED_B 0xB0000000u
#define SEED_C 0xC0000000u

/* generation C: forked from B. B's diverged range is [0, TENTH).
 * C will diverge its own range [TENTH, 2*TENTH). */
static int run_gen_c(unsigned char *buf) {
    if (!cow_verify_range(buf, 0, TENTH, SEED_B, "C: B's range")) return 1;
    if (!cow_verify_range(buf, TENTH, SIZE - TENTH, SEED_A, "C: still-original range")) return 1;

    cow_fill_range(buf, TENTH, TENTH, SEED_C);
    if (!cow_verify_range(buf, TENTH, TENTH, SEED_C, "C: own write")) return 1;
    return 0;
}

/* generation B: forked from A. Diverges [0, TENTH), then forks C. */
static int run_gen_b(unsigned char *buf) {
    if (!cow_verify(buf, SIZE, SEED_A, "B: inherited from A")) return 1;

    cow_fill_range(buf, 0, TENTH, SEED_B);
    if (!cow_verify_range(buf, 0, TENTH, SEED_B, "B: own write")) return 1;
    if (!cow_verify_range(buf, TENTH, SIZE - TENTH, SEED_A, "B: rest still A's")) return 1;

    pid_t pid_c = fork();
    if (pid_c < 0) return 1;
    if (pid_c == 0) {
        _exit(run_gen_c(buf));
    }

    int status;
    if (waitpid(pid_c, &status, 0) != pid_c) return 1;
    if (!(WIFEXITED(status) && WEXITSTATUS(status) == 0)) return 1;

    /* B's own view must be unaffected by C's write */
    if (!cow_verify_range(buf, 0, TENTH, SEED_B, "B-after-C: own range")) return 1;
    if (!cow_verify_range(buf, TENTH, SIZE - TENTH, SEED_A, "B-after-C: rest")) return 1;
    return 0;
}

int main(void) {
    unsigned char *buf = malloc(SIZE);
    COW_CHECK(buf != NULL, "malloc failed");
    cow_fill(buf, SIZE, SEED_A);

    pid_t pid_b = fork();
    COW_CHECK(pid_b >= 0, "fork A->B failed");
    if (pid_b == 0) {
        _exit(run_gen_b(buf));
    }

    int status;
    COW_CHECK(waitpid(pid_b, &status, 0) == pid_b, "waitpid B failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "generation B/C chain failed");

    /* A's own view must be fully pristine after both B and C diverged
     * their own copies -- no cross-generation leakage back to the root. */
    COW_CHECK(cow_verify(buf, SIZE, SEED_A, "A-final"), "A's memory corrupted by descendants");

    free(buf);
    return 0;
}
