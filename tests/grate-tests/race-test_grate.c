/* race-test-grate: Minimal grate to reproduce the get_cage() race in lind-wasm.
 *
 * The runtime race:
 *   Thread A (last thread of cage): exit_syscall → remove_cage(id) [line 391]
 *   Thread B (concurrent):          mmap/signal → get_cage(id).unwrap() → PANIC
 *
 * This grate intercepts mmap (syscall 9) — one of the crash sites — and
 * forwards it via make_threei_call. The round-trip through the grate dispatch
 * adds enough latency to widen the race window between cage removal and
 * concurrent cage access.
 *
 * Pair with test/race_test.c which rapidly forks and spawns threads.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <sys/wait.h>
#include <unistd.h>
#include <lind_syscall.h>

#define MMAP_SYSCALL  9
#define FUTEX_SYSCALL 202

/* ── Dispatcher ─────────────────────────────────────────────────────── */

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0)
        return -1;

    typedef int (*handler_t)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                             uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                             uint64_t, uint64_t, uint64_t);

    handler_t fn = (handler_t)(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage,
              arg4, arg4cage, arg5, arg5cage, arg6, arg6cage);
}

/* ── Handlers ───────────────────────────────────────────────────────── */

/* Intercept mmap — the crash site at fs_calls.rs:795.
 * Just forward it. The grate dispatch round-trip is the delay. */
int mmap_handler(uint64_t cageid,
                 uint64_t arg1, uint64_t arg1cage,
                 uint64_t arg2, uint64_t arg2cage,
                 uint64_t arg3, uint64_t arg3cage,
                 uint64_t arg4, uint64_t arg4cage,
                 uint64_t arg5, uint64_t arg5cage,
                 uint64_t arg6, uint64_t arg6cage) {
    int thiscage = getpid();
    return make_threei_call(MMAP_SYSCALL, 0,
                            thiscage, thiscage,
                            arg1, arg1cage, arg2, arg2cage,
                            arg3, arg3cage, arg4, arg4cage,
                            arg5, arg5cage, arg6, arg6cage, 0);
}

/* Intercept futex — high-frequency during thread sync, adds more pressure. */
int futex_handler(uint64_t cageid,
                  uint64_t arg1, uint64_t arg1cage,
                  uint64_t arg2, uint64_t arg2cage,
                  uint64_t arg3, uint64_t arg3cage,
                  uint64_t arg4, uint64_t arg4cage,
                  uint64_t arg5, uint64_t arg5cage,
                  uint64_t arg6, uint64_t arg6cage) {
    int thiscage = getpid();
    return make_threei_call(FUTEX_SYSCALL, 0,
                            thiscage, thiscage,
                            arg1, arg1cage, arg2, arg2cage,
                            arg3, arg3cage, arg4, arg4cage,
                            arg5, arg5cage, arg6, arg6cage, 0);
}

/* ── Main ───────────────────────────────────────────────────────────── */

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_binary> [args...]\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    int grateid = getpid();
    pid_t pid = fork();

    if (pid < 0) {
        perror("fork failed");
        exit(EXIT_FAILURE);
    } else if (pid == 0) {
        int cageid = getpid();

        register_handler(cageid, MMAP_SYSCALL, grateid,
                         (uint64_t)(uintptr_t)&mmap_handler);
        register_handler(cageid, FUTEX_SYSCALL, grateid,
                         (uint64_t)(uintptr_t)&futex_handler);

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            exit(EXIT_FAILURE);
        }
    }

    int status;
    while (wait(&status) > 0)
        ;

    return 0;
}
