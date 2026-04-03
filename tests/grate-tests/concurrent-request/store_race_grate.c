/* store_race_grate.c — Grate for concurrent Store access test (#961).
 *
 * Interposes on geteuid (syscall 107) and does heap-heavy work in the
 * handler: lazy-init shared structures, realloc under contention,
 * ring-buffer malloc/free churn. This exercises the same patterns as a
 * real Rust grate (fdtables/DashMap) under concurrent access.
 *
 * Without the fix for #961 (separate Store per pool entry), this test
 * should crash with index-out-of-bounds in func.rs, memory faults, or
 * other Store corruption symptoms.
 */
#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[store_race] Invalid function ptr\n");
        assert(0);
    }

    int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid);
}

/* Persistent shared state — simulates what a real grate maintains
 * across calls (e.g. fdtables DashMap, logging buffers, counters). */
static int *call_counts = NULL;     /* per-cage call counter array */
static int call_counts_cap = 0;
static char **log_ring = NULL;      /* circular log buffer */
static int log_ring_size = 64;
static int log_next = 0;

/* Simulate what a real grate handler does: persistent heap state,
 * growing data structures, lookups, and writes to shared memory.
 * This exercises dlmalloc contention, shared global mutation, and
 * pointer chasing — the same patterns as fdtables/DashMap in Rust. */
int store_race_handler(uint64_t cageid) {
    int id = (int)cageid;

    /* Lazy init of shared state — races here mirror DashMap lazy init */
    if (!call_counts) {
        call_counts_cap = 64;
        call_counts = calloc(call_counts_cap, sizeof(int));
        if (!call_counts) return -1;
    }

    if (!log_ring) {
        log_ring = calloc(log_ring_size, sizeof(char *));
        if (!log_ring) return -1;
    }

    /* Grow the counter array if needed — realloc under contention */
    if (id >= call_counts_cap) {
        int new_cap = call_counts_cap * 2;
        while (id >= new_cap) new_cap *= 2;
        int *new_counts = realloc(call_counts, new_cap * sizeof(int));
        if (!new_counts) return -1;
        for (int i = call_counts_cap; i < new_cap; i++)
            new_counts[i] = 0;
        call_counts = new_counts;
        call_counts_cap = new_cap;
    }

    /* Increment per-cage counter — shared write */
    call_counts[id]++;

    /* Allocate a log entry, write to it, store in ring buffer, free old */
    int slot = log_next % log_ring_size;
    log_next++;

    char *entry = malloc(128);
    if (!entry) return -1;
    snprintf(entry, 128, "cage=%d call=%d", id, call_counts[id]);

    char *old = log_ring[slot];
    log_ring[slot] = entry;
    free(old);  /* free previous entry in this slot */

    return 10;
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_binary> [args...]\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();

    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        assert(0);
    } else if (pid == 0) {
        int cageid = getpid();
        uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&store_race_handler;
        printf("[store_race] Registering handler for cage %d "
               "in grate %d with fn ptr addr: %llu\n",
               cageid, grateid, fn_ptr_addr);
        register_handler(cageid, 107, grateid, fn_ptr_addr);

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    int status;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr, "[store_race] FAIL: child exited with status %d\n",
                    status);
            assert(0);
        }
    }

    printf("[store_race] PASS\n");
    return 0;
}
