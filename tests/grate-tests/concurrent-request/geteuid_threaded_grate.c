/* geteuid_threaded_grate.c — Grate for concurrent Store access test (#961).
 *
 * Identical to geteuid_grate.c but paired with geteuid_threaded.c which
 * uses multiple threads. This test triggers concurrent
 * grate_callback_trampoline calls into the same Wasmtime Store.
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
        fprintf(stderr, "[Grate|geteuid] Invalid function ptr\n");
        assert(0);
    }

    int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid);
}

int geteuid_grate(uint64_t cageid) {
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
        uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&geteuid_grate;
        printf("[Grate|geteuid_threaded] Registering geteuid handler for cage %d "
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
            fprintf(stderr, "[Grate|geteuid_threaded] FAIL: child exited with status %d\n",
                    status);
            assert(0);
        }
    }

    printf("[Grate|geteuid_threaded] PASS\n");
    return 0;
}
