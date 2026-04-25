/*
 * test_stack_depth.c — standalone stress test for stack depth checking in wasm
 *
 * Replicates the pattern from PostgreSQL's expression_tree_mutator_impl:
 * deep recursion with check_stack_depth() called at each level.
 *
 * On native Linux, check_stack_depth() detects the overflow and prints an
 * error before the hardware limit is hit.  On wasm, we want to verify
 * the same behavior — if it doesn't work, the process traps with a
 * stack overflow instead of a clean error.
 *
 * Build (native):
 *   gcc -O2 -o test_stack_depth test_stack_depth.c
 *
 * Build (wasm via lind-wasm-apps toolchain):
 *   $CLANG --target=wasm32-unknown-wasi --sysroot=$SYSROOT \
 *     -O2 -g -pthread \
 *     -Wl,--import-memory,--export-memory,--max-memory=67108864 \
 *     -Wl,--export=__stack_pointer,--export=__stack_low \
 *     -Wl,-z,stack-size=8388608 \
 *     -o test_stack_depth test_stack_depth.c
 *
 * Expected: prints "PASS: stack depth limit caught at depth N" and exits 0.
 * Failure:  process traps/crashes (stack overflow not detected).
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <limits.h>

/* ---------- Minimal reimplementation of PG stack depth checking ---------- */

static char *stack_base_ptr = NULL;
static long max_stack_depth_bytes = 100 * 1024;  /* 100 KB default, same as PG */

static void set_stack_base(void)
{
#ifdef __GNUC__
    stack_base_ptr = __builtin_frame_address(0);
#else
    char local;
    stack_base_ptr = &local;
#endif
    printf("[info] stack_base_ptr = %p\n", (void *)stack_base_ptr);
}

static int stack_is_too_deep(void)
{
    char stack_top_loc;
    long stack_depth;

    stack_depth = (long)(stack_base_ptr - &stack_top_loc);
    if (stack_depth < 0)
        stack_depth = -stack_depth;

    if (stack_depth > max_stack_depth_bytes && stack_base_ptr != NULL)
        return 1;

    return 0;
}

static long get_current_stack_depth(void)
{
    char stack_top_loc;
    long depth = (long)(stack_base_ptr - &stack_top_loc);
    return depth < 0 ? -depth : depth;
}

static long get_stack_rlimit(void)
{
    struct rlimit rlim;
    if (getrlimit(RLIMIT_STACK, &rlim) < 0)
        return -1;
    if (rlim.rlim_cur == RLIM_INFINITY)
        return -1;
    return (long)rlim.rlim_cur;
}

/* ---------- Recursive function mimicking expression_tree_mutator ---------- */

/*
 * Each call uses ~256 bytes of stack (local buffer + frame overhead).
 * This simulates the stack usage of a real recursive tree walker.
 */
static volatile int caught_overflow = 0;
static volatile int max_depth_reached = 0;

static void recursive_mutator(int depth)
{
    /* Local buffer to simulate real stack frame size */
    volatile char frame_payload[128];
    memset((char *)frame_payload, (char)depth, sizeof(frame_payload));

    /* This is the check_stack_depth() call that PG does */
    if (stack_is_too_deep()) {
        caught_overflow = 1;
        max_depth_reached = depth;
        /* In real PG this would be ereport(ERROR, ...) */
        return;
    }

    /* Recurse deeper — mimics mutator(child_node, context) */
    recursive_mutator(depth + 1);
}

/* ---------- Tests ---------- */

static int test_basic_depth_check(void)
{
    printf("\n=== Test 1: Basic stack depth overflow detection ===\n");

    set_stack_base();
    caught_overflow = 0;
    max_depth_reached = 0;

    long rlimit = get_stack_rlimit();
    printf("[info] RLIMIT_STACK = %ld bytes (%ld KB)\n",
           rlimit, rlimit > 0 ? rlimit / 1024 : -1);
    printf("[info] max_stack_depth_bytes = %ld bytes (%ld KB)\n",
           max_stack_depth_bytes, max_stack_depth_bytes / 1024);

    recursive_mutator(0);

    if (caught_overflow) {
        long depth_at_catch = get_current_stack_depth();
        printf("[PASS] Stack depth limit caught at recursion depth %d\n",
               max_depth_reached);
        printf("[info] Current stack depth after unwind: %ld bytes\n",
               depth_at_catch);
        return 0;
    } else {
        printf("[FAIL] Recursion returned without catching overflow!\n");
        return 1;
    }
}

static int test_deep_recursion_various_limits(void)
{
    printf("\n=== Test 2: Various max_stack_depth limits ===\n");

    long limits[] = { 32 * 1024, 64 * 1024, 100 * 1024, 512 * 1024, 1024 * 1024 };
    int n = sizeof(limits) / sizeof(limits[0]);
    int failures = 0;

    for (int i = 0; i < n; i++) {
        set_stack_base();
        max_stack_depth_bytes = limits[i];
        caught_overflow = 0;
        max_depth_reached = 0;

        recursive_mutator(0);

        if (caught_overflow) {
            printf("[PASS] limit=%6ldKB -> caught at depth %d\n",
                   limits[i] / 1024, max_depth_reached);
        } else {
            printf("[FAIL] limit=%6ldKB -> NOT caught!\n",
                   limits[i] / 1024);
            failures++;
        }
    }

    return failures;
}

/*
 * Test 4: Simulate postgres-like rlimit-based stack depth limit.
 *
 * Postgres sets max_stack_depth based on getrlimit(RLIMIT_STACK),
 * typically to rlimit - 512KB safety margin. This test replicates
 * that logic and verifies the check catches overflow before the
 * real stack is exhausted.
 */
static int test_rlimit_based_limit(void)
{
    printf("\n=== Test 4: Postgres-like rlimit-based depth limit ===\n");

    long rlimit = get_stack_rlimit();
    printf("[info] RLIMIT_STACK reports: %ld bytes (%ld KB)\n",
           rlimit, rlimit > 0 ? rlimit / 1024 : -1);

    if (rlimit <= 0) {
        printf("[SKIP] getrlimit returned %ld, cannot test rlimit-based limit\n", rlimit);
        return 0;
    }

    /* Postgres default: max_stack_depth = min(2MB, rlimit - 512KB) */
    long pg_default = 2 * 1024 * 1024;
    long pg_max = rlimit - 512 * 1024;
    if (pg_max < pg_default)
        pg_default = pg_max;

    set_stack_base();
    max_stack_depth_bytes = pg_default;
    caught_overflow = 0;
    max_depth_reached = 0;

    printf("[info] Using postgres-like limit: %ld bytes (%ld KB)\n",
           max_stack_depth_bytes, max_stack_depth_bytes / 1024);

    recursive_mutator(0);

    if (caught_overflow) {
        printf("[PASS] Caught at depth %d with postgres-like limit\n",
               max_depth_reached);
        return 0;
    } else {
        printf("[FAIL] Not caught with postgres-like limit!\n");
        return 1;
    }
}

/*
 * Test 5: Stress test with limits approaching real stack size.
 *
 * Tests limits at 25%, 50%, 75%, and 90% of the reported rlimit
 * to find where the check stops working.
 */
static int test_approaching_real_limit(void)
{
    printf("\n=== Test 5: Limits approaching real stack size ===\n");

    long rlimit = get_stack_rlimit();
    if (rlimit <= 0) {
        printf("[SKIP] getrlimit returned %ld\n", rlimit);
        return 0;
    }

    int percentages[] = { 25, 50, 75, 90 };
    int n = sizeof(percentages) / sizeof(percentages[0]);
    int failures = 0;

    for (int i = 0; i < n; i++) {
        set_stack_base();
        max_stack_depth_bytes = (rlimit * percentages[i]) / 100;
        caught_overflow = 0;
        max_depth_reached = 0;

        printf("[info] Testing %d%% of rlimit = %ld bytes (%ld KB)... ",
               percentages[i], max_stack_depth_bytes, max_stack_depth_bytes / 1024);
        fflush(stdout);

        recursive_mutator(0);

        if (caught_overflow) {
            printf("PASS (depth %d)\n", max_depth_reached);
        } else {
            printf("FAIL\n");
            failures++;
        }
    }

    return failures;
}

/*
 * Test 6: Repeated recursion cycles.
 *
 * Postgres handles many queries, each of which may recurse deeply.
 * Verify the check works correctly across multiple recursion cycles
 * without state leaking between them.
 */
static int test_repeated_recursion(void)
{
    printf("\n=== Test 6: Repeated recursion cycles ===\n");

    int failures = 0;

    for (int round = 0; round < 100; round++) {
        set_stack_base();
        max_stack_depth_bytes = 100 * 1024;
        caught_overflow = 0;
        max_depth_reached = 0;

        recursive_mutator(0);

        if (!caught_overflow) {
            printf("[FAIL] Round %d: overflow not caught\n", round);
            failures++;
            break;
        }
    }

    if (failures == 0)
        printf("[PASS] 100 recursion cycles all caught correctly\n");

    return failures;
}

static int test_stack_pointer_sanity(void)
{
    printf("\n=== Test 3: Stack pointer sanity ===\n");

    set_stack_base();
    char local;
    long depth = (long)(stack_base_ptr - &local);

    printf("[info] stack_base_ptr  = %p\n", (void *)stack_base_ptr);
    printf("[info] &local          = %p\n", (void *)&local);
    printf("[info] raw difference  = %ld bytes\n", depth);
    printf("[info] abs difference  = %ld bytes\n", depth < 0 ? -depth : depth);

    /* The difference should be small (we just set the base) */
    long abs_depth = depth < 0 ? -depth : depth;
    if (abs_depth < 4096) {
        printf("[PASS] Initial stack depth is sane (%ld bytes from base)\n",
               abs_depth);
        return 0;
    } else {
        printf("[FAIL] Initial stack depth is suspicious (%ld bytes from base)\n",
               abs_depth);
        return 1;
    }
}

/* ---------- Main ---------- */

int main(void)
{
    int failures = 0;

    printf("=== PostgreSQL stack depth check stress test ===\n");
    printf("Replicates check_stack_depth() from expression_tree_mutator_impl\n");

    failures += test_stack_pointer_sanity();
    failures += test_basic_depth_check();
    failures += test_deep_recursion_various_limits();
    failures += test_rlimit_based_limit();
    failures += test_approaching_real_limit();
    failures += test_repeated_recursion();

    printf("\n=== Summary: %d failure(s) ===\n", failures);
    return failures > 0 ? 1 : 0;
}
