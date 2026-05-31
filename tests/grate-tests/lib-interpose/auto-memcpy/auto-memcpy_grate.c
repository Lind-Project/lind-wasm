// Grate for auto-memcpy marshalling test.
// Intercepts libc memcpy using lind_marshal.h automated marshalling:
//   arg0: PTR OUT  sized by arg2   (dest buffer)
//   arg1: PTR IN   sized by arg2   (src buffer)
//   arg2: SCALAR                   (n)
//   ret:  PTR_ALIAS_ARG 0          (returns original source-cage dest)
//
// The handler calls the real memcpy on shadow buffers — no manual
// copy_data_between_cages needed.
//
// Compile:
//   lind-clang -s --compile-grate \
//     -I../../ \
//     auto-memcpy_grate.c -o auto-memcpy_grate.wasm
//
// Run (from lindfs/):
//   lind-wasm grates/auto-memcpy_grate.cwasm /auto-memcpy.cwasm
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

// ---------------------------------------------------------------------------
// Spec and handler
// ---------------------------------------------------------------------------

static struct lind_marshal_spec memcpy_spec = {
    .nargs = 3,
    .args = {
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_OUT,
            .size_kind     = LIND_SIZE_FROM_ARG,
            .size_arg_index = 2,
        },
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_IN,
            .size_kind     = LIND_SIZE_FROM_ARG,
            .size_arg_index = 2,
        },
        { .kind = LIND_ARG_SCALAR },
    },
    .ret = { .kind = LIND_RET_PTR_ALIAS_ARG, .alias_arg_index = 0 },
};

static uint64_t handler_memcpy(uint64_t dest, uint64_t src, uint64_t n,
                                uint64_t, uint64_t, uint64_t) {
    printf("[Grate|auto-memcpy] memcpy intercepted: n=%zu\n", LIND_AS_SIZE(n));
    return LIND_RET_PTR(memcpy(LIND_AS_PTR(dest), LIND_AS_CPTR(src), LIND_AS_SIZE(n)));
}

LIND_DEFINE_MARSHAL_HANDLER(memcpy, &memcpy_spec, handler_memcpy)

// ---------------------------------------------------------------------------
// Standard grate dispatcher (required export)
// ---------------------------------------------------------------------------

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|auto-memcpy] invalid fn ptr\n");
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

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_wasm>\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); assert(0); }

    if (pid == 0) {
        int cageid = getpid();
        printf("[Grate|auto-memcpy] registering memcpy handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "memcpy",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_memcpy);
        if (ret != 0) {
            fprintf(stderr, "[Grate|auto-memcpy] register memcpy failed: %d\n", ret);
            assert(0);
        }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }

    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;

    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-memcpy] FAIL: cage exited with %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-memcpy] PASS\n");
    return 0;
}
