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
        fprintf(stderr, "[Grate|lib-interpose] invalid fn ptr\n");
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

// Handler for toy_add: receives (cageid, a, 0, b, 0, ...).
// Returns (a + b) * 2 to prove the call was intercepted (real result would be a + b).
int toy_add_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    int a = (int)arg1;
    int b = (int)arg2;
    printf("[Grate|lib-interpose] toy_add_handler: cage=%llu a=%d b=%d\n",
           (unsigned long long)cageid, a, b);
    return (a + b) * 2;  // 3+4=7, *2=14
}

// Handler for toy_mul: returns a + b instead of a * b to prove interposition.
int toy_mul_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    int a = (int)arg1;
    int b = (int)arg2;
    printf("[Grate|lib-interpose] toy_mul_handler: cage=%llu a=%d b=%d\n",
           (unsigned long long)cageid, a, b);
    return a + b;  // 5+6=11 instead of 5*6=30
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
        // Child: register lib handlers for the cage, then exec into the cage app.
        int cageid = getpid();

        uint64_t add_fn = (uint64_t)(uintptr_t)&toy_add_handler;
        uint64_t mul_fn = (uint64_t)(uintptr_t)&toy_mul_handler;

        printf("[Grate|lib-interpose] registering toy_add handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "toy_add",
                                       LIBCALL_BASE + 1, grateid, add_fn);
        if (ret != 0) {
            fprintf(stderr, "[Grate|lib-interpose] register toy_add failed: %d\n", ret);
            assert(0);
        }

        printf("[Grate|lib-interpose] registering toy_mul handler for cage %d\n", cageid);
        ret = register_lib_handler(cageid, "env", "toy_mul",
                                   LIBCALL_BASE + 2, grateid, mul_fn);
        if (ret != 0) {
            fprintf(stderr, "[Grate|lib-interpose] register toy_mul failed: %d\n", ret);
            assert(0);
        }

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    // Parent: wait for cage to finish.
    int status;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr, "[Grate|lib-interpose] FAIL: child exited with status %d\n", status);
            assert(0);
        }
    }

    printf("[Grate|lib-interpose] PASS\n");
    return 0;
}
