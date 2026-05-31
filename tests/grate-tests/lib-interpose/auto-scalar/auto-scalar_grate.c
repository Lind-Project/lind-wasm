// Grate for auto-scalar marshalling test.
// Intercepts toy_add(a, b) from libtoy using lind_marshal.h automated
// marshalling with SCALAR specs.  Returns a*b instead of a+b to prove
// interception and that scalar args are passed correctly to the handler.
//
// Compile:
//   lind-clang -s --compile-grate \
//     -I../../ \
//     auto-scalar_grate.c -o auto-scalar_grate.wasm
//
// Run (from lindfs/):
//   lind-wasm --preload env=/lib/libtoy.cwasm \
//             grates/auto-scalar_grate.cwasm /auto-scalar.cwasm
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

// ---------------------------------------------------------------------------
// Handler: receives toy_add(a, b) as shadow-marshalled scalars.
// Returns a * b to prove interception.
// ---------------------------------------------------------------------------

static struct lind_marshal_spec toy_add_spec = {
    .nargs = 2,
    .args = {
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_toy_add(uint64_t a, uint64_t b,
                                 uint64_t, uint64_t, uint64_t, uint64_t) {
    int ia = LIND_AS_INT(a);
    int ib = LIND_AS_INT(b);
    printf("[Grate|auto-scalar] toy_add intercepted: a=%d b=%d -> returning %d (a*b)\n",
           ia, ib, ia * ib);
    return LIND_RET_INT(ia * ib);
}

LIND_DEFINE_MARSHAL_HANDLER(toy_add, &toy_add_spec, handler_toy_add)

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
        fprintf(stderr, "[Grate|auto-scalar] invalid fn ptr\n");
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
// main: register handlers and exec the cage
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
        printf("[Grate|auto-scalar] registering toy_add handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "toy_add",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_toy_add);
        if (ret != 0) {
            fprintf(stderr, "[Grate|auto-scalar] register toy_add failed: %d\n", ret);
            assert(0);
        }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }

    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;

    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-scalar] FAIL: cage exited with %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-scalar] PASS\n");
    return 0;
}
