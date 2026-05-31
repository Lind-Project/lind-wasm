// Grate for auto-strncpy marshalling test.
// Intercepts libc strncpy using lind_marshal.h automated marshalling.
// Treats both src and dst as buffers of size n (conservative but safe for
// stage 1 as documented in design-s2.md §5.4 and §6.4).
//
// Compile:
//   lind-clang -s --compile-grate \
//     -I../../ \
//     auto-strncpy_grate.c -o auto-strncpy_grate.wasm
//
// Run (from lindfs/):
//   lind-wasm grates/auto-strncpy_grate.cwasm /auto-strncpy.cwasm
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

static struct lind_marshal_spec strncpy_spec = {
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

static uint64_t handler_strncpy(uint64_t dest, uint64_t src, uint64_t n,
                                 uint64_t, uint64_t, uint64_t) {
    printf("[Grate|auto-strncpy] strncpy intercepted: n=%zu\n", LIND_AS_SIZE(n));
    return LIND_RET_PTR(strncpy(LIND_AS_STR(dest), LIND_AS_CSTR(src), LIND_AS_SIZE(n)));
}

LIND_DEFINE_MARSHAL_HANDLER(strncpy, &strncpy_spec, handler_strncpy)

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
        fprintf(stderr, "[Grate|auto-strncpy] invalid fn ptr\n");
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
        printf("[Grate|auto-strncpy] registering strncpy handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "strncpy",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_strncpy);
        if (ret != 0) {
            fprintf(stderr, "[Grate|auto-strncpy] register strncpy failed: %d\n", ret);
            assert(0);
        }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }

    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;

    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-strncpy] FAIL: cage exited with %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-strncpy] PASS\n");
    return 0;
}
