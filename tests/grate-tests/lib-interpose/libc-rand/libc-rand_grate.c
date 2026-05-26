// Grate for libc-rand interposition test.
// Intercepts rand() from libc and returns a fixed value to prove interposition.
#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

// Standard grate dispatcher — required export in every grate.
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|libc-rand] invalid fn ptr\n");
        assert(0);
    }
    int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage,
              arg5, arg5cage, arg6, arg6cage);
}

// Handler for rand(): ignores the cage's real rand() and always returns 42.
int rand_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    printf("[Grate|libc-rand] rand_handler: cage=%llu — returning 42\n",
           (unsigned long long)cageid);
    return 42;
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_wasm>\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();

    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        assert(0);
    }

    if (pid == 0) {
        int cageid = getpid();

        uint64_t rand_fn = (uint64_t)(uintptr_t)&rand_handler;

        printf("[Grate|libc-rand] registering rand handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "rand",
                                       LIBCALL_BASE + 1, grateid, rand_fn);
        if (ret != 0) {
            fprintf(stderr, "[Grate|libc-rand] register rand failed: %d\n", ret);
            assert(0);
        }

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    int status;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr, "[Grate|libc-rand] FAIL: child exited with status %d\n", status);
            assert(0);
        }
    }

    printf("[Grate|libc-rand] PASS\n");
    return 0;
}
