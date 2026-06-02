// Grate for auto-memchr marshalling test.
// Intercepts memchr with LIND_RET_PTR_INTO_ARG: the handler returns a pointer
// into the shadow buffer; the runtime translates it back to the source-cage
// offset so the cage receives a valid pointer into its own buffer.
//
// Compile:
//   lind-clang -s --compile-grate auto-memchr_grate.c -- -I..
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

static struct lind_marshal_spec memchr_spec = {
    .nargs = 3,
    .args = {
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_IN,
            .size_kind     = LIND_SIZE_FROM_ARG,
            .size_arg_index = 2,
        },
        { .kind = LIND_ARG_SCALAR },  // c (int)
        { .kind = LIND_ARG_SCALAR },  // n (size_t)
    },
    // Return a pointer into arg0's buffer; runtime translates shadow offset
    // back to the source-cage address.
    .ret = { .kind = LIND_RET_PTR_INTO_ARG, .alias_arg_index = 0 },
};

static uint64_t handler_memchr(uint64_t s, uint64_t c, uint64_t n,
                                uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a3; (void)a4; (void)a5;
    void *result = memchr(LIND_AS_CPTR(s), LIND_AS_INT(c), LIND_AS_SIZE(n));
    printf("[Grate|auto-memchr] memchr intercepted, result=%p base=%p\n",
           result, LIND_AS_CPTR(s));
    return LIND_RET_PTR(result);
}

LIND_DEFINE_MARSHAL_HANDLER(memchr, &memchr_spec, handler_memchr)

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) { fprintf(stderr, "[Grate|auto-memchr] invalid fn ptr\n"); assert(0); }
    int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int main(int argc, char *argv[]) {
    if (argc < 2) { fprintf(stderr, "Usage: %s <cage>\n", argv[0]); assert(0); }
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); assert(0); }
    if (pid == 0) {
        int cageid = getpid();
        printf("[Grate|auto-memchr] registering memchr for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "memchr",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_memchr);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-memchr] register failed: %d\n", ret); assert(0); }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-memchr] FAIL: cage exited %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-memchr] PASS\n");
    return 0;
}
