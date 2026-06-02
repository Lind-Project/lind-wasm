// Grate for auto-handle marshalling test.
// Intercepts toy_ctx_create/get_val/close from libtoy using the handle table.
//
//   toy_ctx_create(val) → handler allocates a real ctx in grate memory,
//                         registers it, returns app_token (LIND_RET_HANDLE)
//   toy_ctx_get_val(token) → LIND_ARG_HANDLE translates token → real ctx*;
//                            handler returns ctx->val
//   toy_ctx_close(token)   → LIND_ARG_HANDLE translates; handler frees; releases table entry
//
// Compile:
//   lind-clang -s --compile-grate auto-handle_grate.c -- -I..
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

#define CTX_CLASS 1

struct _ctx { int val; };

// --- toy_ctx_create(int val) → RET_HANDLE ---

static struct lind_marshal_spec ctx_create_spec = {
    .nargs = 1,
    .args  = { { .kind = LIND_ARG_SCALAR } },
    .ret   = { .kind = LIND_RET_HANDLE, .handle_class = CTX_CLASS },
};

static uint64_t handler_ctx_create(uint64_t val, uint64_t a1, uint64_t a2,
                                    uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    struct _ctx *ctx = (struct _ctx *)malloc(sizeof(struct _ctx));
    ctx->val = LIND_AS_INT(val);
    printf("[Grate|auto-handle] toy_ctx_create(%d) -> real_ptr=%p\n",
           ctx->val, (void *)ctx);
    // Return the real pointer; lind_marshal_dispatch will register it and
    // return the app_token to the source cage (via LIND_RET_HANDLE).
    return (uint64_t)(uintptr_t)ctx;
}

LIND_DEFINE_MARSHAL_HANDLER(toy_ctx_create, &ctx_create_spec, handler_ctx_create)

// --- toy_ctx_get_val(token) → scalar ---

static struct lind_marshal_spec ctx_get_val_spec = {
    .nargs = 1,
    .args  = { { .kind = LIND_ARG_HANDLE, .handle_class = CTX_CLASS } },
    .ret   = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_ctx_get_val(uint64_t real_ptr, uint64_t a1, uint64_t a2,
                                     uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    struct _ctx *ctx = (struct _ctx *)(uintptr_t)real_ptr;
    printf("[Grate|auto-handle] toy_ctx_get_val(ptr=%p) -> %d\n", (void *)ctx, ctx->val);
    return LIND_RET_INT(ctx->val);
}

LIND_DEFINE_MARSHAL_HANDLER(toy_ctx_get_val, &ctx_get_val_spec, handler_ctx_get_val)

// --- toy_ctx_close(token) → void ---

static struct lind_marshal_spec ctx_close_spec = {
    .nargs = 1,
    .args  = { { .kind = LIND_ARG_HANDLE, .handle_class = CTX_CLASS } },
    .ret   = { .kind = LIND_RET_VOID },
};

static uint64_t handler_ctx_close(uint64_t real_ptr, uint64_t a1, uint64_t a2,
                                   uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    struct _ctx *ctx = (struct _ctx *)(uintptr_t)real_ptr;
    printf("[Grate|auto-handle] toy_ctx_close(ptr=%p)\n", (void *)ctx);
    free(ctx);
    // Also release from handle table. We need the original app_token; look it up
    // by real_ptr (linear scan is fine for tests).
    for (int i = 0; i < LIND_HANDLE_TABLE_SIZE; i++) {
        if (_lind_handle_table[i].in_use &&
            _lind_handle_table[i].real_object == real_ptr &&
            _lind_handle_table[i].handle_class == CTX_CLASS) {
            _lind_handle_table[i].in_use = 0;
            break;
        }
    }
    return 0;
}

LIND_DEFINE_MARSHAL_HANDLER(toy_ctx_close, &ctx_close_spec, handler_ctx_close)

// ---------------------------------------------------------------------------

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) { fprintf(stderr, "[Grate|auto-handle] invalid fn ptr\n"); assert(0); }
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
        printf("[Grate|auto-handle] registering ctx handlers for cage %d\n", cageid);
        int ret;
        ret = register_lib_handler(cageid, "env", "toy_ctx_create",
                  grateid, (uint64_t)(uintptr_t)&lind_mh_toy_ctx_create);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-handle] register create failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_ctx_get_val",
                  grateid, (uint64_t)(uintptr_t)&lind_mh_toy_ctx_get_val);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-handle] register get_val failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_ctx_close",
                  grateid, (uint64_t)(uintptr_t)&lind_mh_toy_ctx_close);
        if (ret != 0) { fprintf(stderr, "[Grate|auto-handle] register close failed\n"); assert(0); }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (child_exit != 0) {
        fprintf(stderr, "[Grate|auto-handle] FAIL: cage exited %d\n", child_exit);
        return 1;
    }
    printf("[Grate|auto-handle] PASS\n");
    return 0;
}
