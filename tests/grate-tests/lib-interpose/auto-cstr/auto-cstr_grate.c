// Grate for auto-cstr marshalling test.
// Intercepts strlen() with LIND_SIZE_CSTR: the runtime scans for '\0' in the
// source cage, allocates a shadow copy, and passes it to the handler.
// Handler returns len*2 to prove the string was accessible locally.
//
// Compile:
//   lind-clang -s --compile-grate auto-cstr_grate.c -o auto-cstr_grate.wasm -- -I..
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

static struct lind_marshal_spec strlen_spec = {
    .nargs = 1,
    .args = {
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_IN,
            .size_kind     = LIND_SIZE_CSTR,
        },
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_strlen(uint64_t s, uint64_t a1, uint64_t a2,
                                uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    size_t len = strlen(LIND_AS_CSTR(s));
    printf("[Grate|auto-cstr] strlen intercepted: \"%s\" len=%zu -> returning %zu\n",
           LIND_AS_CSTR(s), len, len * 2);
    return (uint64_t)(len * 2);
}

LIND_DEFINE_MARSHAL_HANDLER(strlen, &strlen_spec, handler_strlen)

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) { fprintf(stderr, "[Grate|auto-cstr] invalid fn ptr\n"); assert(0); }
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
        printf("[Grate|auto-cstr] registering strlen handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "strlen",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_strlen);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-cstr] register failed: %d\n", ret); assert(0); }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-cstr] FAIL: cage exited %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-cstr] PASS\n");
    return 0;
}
