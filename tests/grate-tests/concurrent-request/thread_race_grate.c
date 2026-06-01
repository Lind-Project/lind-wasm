/* thread_race_grate.c — Thread-safe grate for concurrent callback test .
 *
 * Interposes on geteuid (syscall 107). This version intentionally keeps
 * heap-heavy work in the handler, but protects all shared state with a mutex,
 * so crashes are less likely to come from the grate's own data races.
 *
 * Pair with: thread_race.c
 */
#include <errno.h>
#include <lind_syscall.h>
#include <pthread.h>
#include <stdint.h>
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
        fprintf(stderr, "[thread_race] Invalid function ptr\n");
        assert(0);
    }

    int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid);
}

/* Persistent shared state — simulates what a real grate maintains
 * across calls (e.g. fdtables / shared buffers / counters). */
static int *call_counts = NULL;     /* per-cage call counter array */
static int call_counts_cap = 0;

static char **log_ring = NULL;      /* circular log buffer */
static int log_ring_size = 64;
static int log_next = 0;

/* Protect all shared mutable state in the grate. */
static pthread_mutex_t grate_mu = PTHREAD_MUTEX_INITIALIZER;

/* Thread-safe heavy handler:
 * - lazy init shared structures
 * - realloc under growth
 * - shared counter mutation
 * - malloc/free churn in a ring buffer
 *
 * If this version still crashes under concurrent callbacks, the issue is
 * less likely to be caused by the handler's own heap races.
 */
int thread_race_handler(uint64_t cageid) {
    int id = (int)cageid;
    int slot;
    char *entry = NULL;
    char *old = NULL;

    if (pthread_mutex_lock(&grate_mu) != 0) {
        return -100;
    }

    if (call_counts == NULL) {
        call_counts_cap = 64;
        call_counts = calloc((size_t)call_counts_cap, sizeof(int));
        if (call_counts == NULL) {
            pthread_mutex_unlock(&grate_mu);
            return -101;
        }
    }

    if (log_ring == NULL) {
        log_ring = calloc((size_t)log_ring_size, sizeof(char *));
        if (log_ring == NULL) {
            pthread_mutex_unlock(&grate_mu);
            return -102;
        }
    }

    if (id < 0) {
        pthread_mutex_unlock(&grate_mu);
        return -103;
    }

    if (id >= call_counts_cap) {
        int new_cap = call_counts_cap;
        if (new_cap <= 0) {
            new_cap = 64;
        }

        while (id >= new_cap) {
            if (new_cap > (1 << 29)) {
                pthread_mutex_unlock(&grate_mu);
                return -104;
            }
            new_cap *= 2;
        }

        int *new_counts = realloc(call_counts, (size_t)new_cap * sizeof(int));
        if (new_counts == NULL) {
            pthread_mutex_unlock(&grate_mu);
            return -105;
        }

        for (int i = call_counts_cap; i < new_cap; i++) {
            new_counts[i] = 0;
        }

        call_counts = new_counts;
        call_counts_cap = new_cap;
    }

    call_counts[id]++;

    slot = log_next % log_ring_size;
    log_next++;

    entry = malloc(128);
    if (entry == NULL) {
        pthread_mutex_unlock(&grate_mu);
        return -106;
    }

    snprintf(entry, 128, "cage=%d call=%d", id, call_counts[id]);

    old = log_ring[slot];
    log_ring[slot] = entry;
    free(old);

    if (pthread_mutex_unlock(&grate_mu) != 0) {
        return -107;
    }

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
        uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&thread_race_handler;

        printf("[thread_race] Registering handler for cage %d "
               "in grate %d with fn ptr addr: %llu\n",
               cageid, grateid, (unsigned long long)fn_ptr_addr);

        register_handler(cageid, 107, grateid, fn_ptr_addr);

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    int status;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr, "[thread_race] FAIL: child exited with status %d\n",
                    status);
            assert(0);
        }
    }

    printf("[thread_race] PASS\n");
    return 0;
}
