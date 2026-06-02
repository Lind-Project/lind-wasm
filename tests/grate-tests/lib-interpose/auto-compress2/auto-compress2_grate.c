// Grate for auto-compress2 marshalling test.
// Demonstrates LIND_SIZE_FROM_ARG_POINTEE: dest size is read from *destLen
// in the source cage before shadow allocation.
//
// compress2(dest, destLen*, source, sourceLen, level)
//   arg0 (dest):      PTR OUT,  size = FROM_ARG_POINTEE[1]  (*destLen)
//   arg1 (destLen):   PTR INOUT, size = CONST 4 (uLongf)
//   arg2 (source):    PTR IN,   size = FROM_ARG[3]
//   arg3 (sourceLen): SCALAR
//   arg4 (level):     SCALAR
//
// Handler writes "LIND" into dest, sets *destLen = 4, returns Z_OK.
//
// Compile:
//   lind-clang -s --compile-grate auto-compress2_grate.c -- -I..
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

static const uint8_t FIXED[] = { 'L', 'I', 'N', 'D' };

static struct lind_marshal_spec compress2_spec = {
    .nargs = 5,
    .args = {
        {   // dest: OUT buffer, size from *destLen (arg1 pointee)
            .kind           = LIND_ARG_PTR,
            .ptr_direction  = LIND_PTR_OUT,
            .size_kind      = LIND_SIZE_FROM_ARG_POINTEE,
            .size_arg_index = 1,
        },
        {   // destLen: INOUT, 4-byte uLongf
            .kind           = LIND_ARG_PTR,
            .ptr_direction  = LIND_PTR_INOUT,
            .size_kind      = LIND_SIZE_CONST,
            .const_size     = 4,
        },
        {   // source: IN buffer, size from sourceLen (arg3)
            .kind           = LIND_ARG_PTR,
            .ptr_direction  = LIND_PTR_IN,
            .size_kind      = LIND_SIZE_FROM_ARG,
            .size_arg_index = 3,
        },
        { .kind = LIND_ARG_SCALAR },  // sourceLen
        { .kind = LIND_ARG_SCALAR },  // level
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_compress2(uint64_t dest, uint64_t destLen,
                                   uint64_t source, uint64_t sourceLen,
                                   uint64_t level, uint64_t a5) {
    (void)source; (void)sourceLen; (void)level; (void)a5;

    // dest and destLen are local shadow pointers.
    memcpy(LIND_AS_PTR(dest), FIXED, 4);
    *(uint32_t *)LIND_AS_PTR(destLen) = 4;

    printf("[Grate|auto-compress2] compress2 intercepted — wrote \"LIND\", destLen=4\n");
    return 0;  // Z_OK
}

LIND_DEFINE_MARSHAL_HANDLER(compress2, &compress2_spec, handler_compress2)

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) { fprintf(stderr, "[Grate|auto-compress2] invalid fn ptr\n"); assert(0); }
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
        printf("[Grate|auto-compress2] registering compress2 for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "compress2",
                      grateid, (uint64_t)(uintptr_t)&lind_mh_compress2);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-compress2] register failed: %d\n", ret); assert(0); }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-compress2] FAIL: cage exited %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-compress2] PASS\n");
    return 0;
}
