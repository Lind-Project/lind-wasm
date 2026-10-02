// Grate exercising lind_marshal.h's LIND_SIZE_EXPR and
// LIND_SIZE_STRIDE_VECTOR's general lind_extent_expr operands
// (size_operand_expr/stride_operand_expr) through the REAL
// lind_marshal_dispatch path -- issue #22's OpenBLAS inference-to-runtime
// integration, Gate 3: "extend generated handler/runtime support".
//
// Gate 1's extentexpr_grate.c already exhaustively covers the shared
// evaluator (_lind_eval_extent_expr/_lind_eval_extent_expr_depth) in
// isolation, by calling it directly from a handler against a trivial
// all-scalar spec. This file tests the NEW plumbing on top of that
// evaluator instead: a real lind_arg_spec PTR field whose size comes from
// a tree, marshalled end to end (shadow allocation, copy-in, the real
// call, copy-out) by _lind_pre_ptr/_lind_compute_size -- the path Gate 1
// never exercised at all.
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

extern int toy_daxpy_exprstride(int n, double alpha, const double *x, int incx,
                                 double *y, int incy);
extern double toy_packedsum_exprsize(int n, const double *ap);
extern int toy_packedsum_badtree(int n, const double *ap);

// ---------------------------------------------------------------------------
// toy_daxpy_exprstride: LIND_SIZE_STRIDE_VECTOR with tree-shaped operands.
// ---------------------------------------------------------------------------
static const struct lind_extent_expr DE_N       = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0, .leaf_type = LIND_EXTENT_LEAF_I32 };
static const struct lind_extent_expr DE_INCX    = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 3, .leaf_type = LIND_EXTENT_LEAF_I32 };
static const struct lind_extent_expr DE_INCY    = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 5, .leaf_type = LIND_EXTENT_LEAF_I32 };
static const struct lind_extent_expr DE_ABS_INCX = { .kind = LIND_EXPR_ABS, .lhs = &DE_INCX };
static const struct lind_extent_expr DE_ABS_INCY = { .kind = LIND_EXPR_ABS, .lhs = &DE_INCY };

static struct lind_marshal_spec daxpy_exprstride_spec = {
    .nargs = 6,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n
        { .kind = LIND_ARG_SCALAR },  // alpha
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand_expr = &DE_N, .stride_operand_expr = &DE_ABS_INCX,
          .const_size = sizeof(double) },  // x
        { .kind = LIND_ARG_SCALAR },  // incx
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_INOUT,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand_expr = &DE_N, .stride_operand_expr = &DE_ABS_INCY,
          .const_size = sizeof(double) },  // y
        { .kind = LIND_ARG_SCALAR },  // incy
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_daxpy_exprstride(uint64_t n, uint64_t alpha_bits, uint64_t x,
                                          uint64_t incx, uint64_t y, uint64_t incy) {
    printf("[Grate|exprsize] toy_daxpy_exprstride handler ran\n");
    fflush(stdout);
    double alpha;
    memcpy(&alpha, &alpha_bits, sizeof(double));
    return LIND_RET_INT(toy_daxpy_exprstride((int)n, alpha, (const double *)LIND_AS_CPTR(x),
                                              (int)(int32_t)incx, (double *)LIND_AS_PTR(y),
                                              (int)(int32_t)incy));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_daxpy_exprstride, &daxpy_exprstride_spec, handler_daxpy_exprstride)

// ---------------------------------------------------------------------------
// toy_packedsum_exprsize: LIND_SIZE_EXPR -- the whole byte extent comes
// from a tree, matching OpenBLAS's real packed-storage formula shape:
// elem_size * ceil_divide(n * (n+1), 2).
// ---------------------------------------------------------------------------
static const struct lind_extent_expr PE_N        = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0, .leaf_type = LIND_EXTENT_LEAF_I32 };
static const struct lind_extent_expr PE_ONE      = { .kind = LIND_EXPR_CONSTANT, .const_value = 1 };
static const struct lind_extent_expr PE_N_PLUS_1 = { .kind = LIND_EXPR_ADD, .lhs = &PE_N, .rhs = &PE_ONE };
static const struct lind_extent_expr PE_N_TIMES_NP1 = { .kind = LIND_EXPR_PRODUCT, .lhs = &PE_N, .rhs = &PE_N_PLUS_1 };
static const struct lind_extent_expr PE_TWO      = { .kind = LIND_EXPR_CONSTANT, .const_value = 2 };
static const struct lind_extent_expr PE_COUNT    = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &PE_N_TIMES_NP1, .rhs = &PE_TWO };
static const struct lind_extent_expr PE_ELEMSIZE = { .kind = LIND_EXPR_CONSTANT, .const_value = sizeof(double) };
static const struct lind_extent_expr PE_BYTES    = { .kind = LIND_EXPR_PRODUCT, .lhs = &PE_ELEMSIZE, .rhs = &PE_COUNT };

static struct lind_marshal_spec packedsum_spec = {
    .nargs = 2,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_EXPR, .size_expr = &PE_BYTES },  // ap
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_packedsum(uint64_t n, uint64_t ap) {
    printf("[Grate|exprsize] toy_packedsum_exprsize handler ran\n");
    fflush(stdout);
    double result = toy_packedsum_exprsize((int)n, (const double *)LIND_AS_CPTR(ap));
    uint64_t bits;
    memcpy(&bits, &result, sizeof(double));
    return bits;
}
LIND_DEFINE_MARSHAL_HANDLER(toy_packedsum_exprsize, &packedsum_spec, handler_packedsum)

// ---------------------------------------------------------------------------
// toy_packedsum_badtree: identical real function, deliberately corrupt
// spec (size_expr's leaf names an argument slot beyond this call's real
// nargs) -- the defense-in-depth counterpart of stridevec_badindex, proving
// a malformed tree still traps via the shared evaluator even though
// generation-time validation (gen_grate.py's _valid_extent_expr_tree) would
// already have refused to emit this spec in the first place.
// ---------------------------------------------------------------------------
static const struct lind_extent_expr BAD_ARG_INDEX = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 99, .leaf_type = LIND_EXTENT_LEAF_I32 };
static const struct lind_extent_expr BAD_ELEMSIZE  = { .kind = LIND_EXPR_CONSTANT, .const_value = sizeof(double) };
static const struct lind_extent_expr BAD_BYTES     = { .kind = LIND_EXPR_PRODUCT, .lhs = &BAD_ELEMSIZE, .rhs = &BAD_ARG_INDEX };

static struct lind_marshal_spec packedsum_badtree_spec = {
    .nargs = 2,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_EXPR, .size_expr = &BAD_BYTES },  // ap
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_packedsum_badtree(uint64_t n, uint64_t ap) {
    printf("[Grate|exprsize] toy_packedsum_badtree handler ran (should not happen)\n");
    fflush(stdout);
    return LIND_RET_INT(toy_packedsum_badtree((int)n, (const double *)LIND_AS_CPTR(ap)));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_packedsum_badtree, &packedsum_badtree_spec, handler_packedsum_badtree)

// ---------------------------------------------------------------------------
// Standard grate dispatcher — required export in every grate.
// ---------------------------------------------------------------------------
int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|exprsize] invalid fn ptr\n");
        assert(0);
    }
    int64_t (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int64_t (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage,
              arg5, arg5cage, arg6, arg6cage);
}

int main(int argc, char *argv[]) {
    if (argc < 2) { fprintf(stderr, "Usage: %s <cage>\n", argv[0]); assert(0); }
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); assert(0); }
    if (pid == 0) {
        int cageid = getpid();
        int ret;
        ret = register_lib_handler(cageid, "env", "toy_daxpy_exprstride", grateid,
                                    (uint64_t)(uintptr_t)&lind_mh_toy_daxpy_exprstride);
        if (ret != 0) { fprintf(stderr, "[Grate|exprsize] register toy_daxpy_exprstride failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_packedsum_exprsize", grateid,
                                    (uint64_t)(uintptr_t)&lind_mh_toy_packedsum_exprsize);
        if (ret != 0) { fprintf(stderr, "[Grate|exprsize] register toy_packedsum_exprsize failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_packedsum_badtree", grateid,
                                    (uint64_t)(uintptr_t)&lind_mh_toy_packedsum_badtree);
        if (ret != 0) { fprintf(stderr, "[Grate|exprsize] register toy_packedsum_badtree failed\n"); assert(0); }

        printf("[Grate|exprsize] registered 3/3 handlers\n");
        fflush(stdout);
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[Grate|exprsize] app exited %d\n", ce);
    return 0;
}
