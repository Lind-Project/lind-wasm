// Grate exercising lind_marshal.h's LIND_SIZE_STRIDE_VECTOR and its
// lind_extent_operand evaluation (issue #26 review): a strided vector
// argument's true byte footprint is (1 + (n-1)*stride) * elem_size, and
// each of n/stride is independently either a direct argument value or a
// value loaded through a pointer argument (the Fortran-style by-reference
// calling convention some numerical libraries use for every scalar).
//
// Four real functions cover the shapes under test:
//   toy_daxpy          n/incx/incy passed by value
//   toy_daxpy_ref       n/incx/incy passed by reference (Fortran-style)
//   toy_daxpy_mixed     n/incy by reference, incx by value
//   toy_daxpy_badindex  identical to toy_daxpy, paired with a spec whose
//                       extent operand arg_index is deliberately invalid
//
// Handlers that call through to a real implementation on the automatically
// marshalled shadow buffers give a correct result in the cage as direct
// evidence the shadow was sized (and copied in/out) correctly, not just
// "large enough to not crash". Handlers for the pointee-sourced n/incx/incy
// dereference them manually (via the same checked cross-cage copy every
// other pointer read in this file uses) purely to obtain a real value to
// call the underlying function with -- that dereference is separate from,
// and does not substitute for, the one lind_marshal_dispatch performs
// itself when evaluating each extent operand for X/Y's own sizing.
//
// A small 4096-byte arena (instead of the default 1MiB) keeps the
// "arenaexhaust" mode's 160000-byte request reliably over capacity
// regardless of what the shared default happens to be, while staying
// comfortably bigger than every other mode's own small (tens-of-bytes)
// buffers.
#define LIND_MARSHAL_ARENA_SIZE 4096
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

extern int toy_daxpy(int n, double alpha, const double *x, int incx, double *y, int incy);
extern int toy_daxpy_ref(const int *n, double alpha, const double *x, const int *incx,
                          double *y, const int *incy);
extern int toy_daxpy_mixed(const int *n, double alpha, const double *x, int incx,
                            double *y, const int *incy);
extern int toy_daxpy_badindex(int n, double alpha, const double *x, int incx, double *y, int incy);

// ---------------------------------------------------------------------------
// toy_daxpy: n/incx/incy by value
// ---------------------------------------------------------------------------
static struct lind_marshal_spec daxpy_spec = {
    .nargs = 6,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n
        { .kind = LIND_ARG_SCALAR },  // alpha
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_VALUE },
          .stride_operand = { .arg_index = 3, .source = LIND_EXTENT_VALUE },
          .const_size = sizeof(double) },  // x
        { .kind = LIND_ARG_SCALAR },  // incx
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_INOUT,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_VALUE },
          .stride_operand = { .arg_index = 5, .source = LIND_EXTENT_VALUE },
          .const_size = sizeof(double) },  // y
        { .kind = LIND_ARG_SCALAR },  // incy
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_daxpy(uint64_t n, uint64_t alpha_bits, uint64_t x,
                               uint64_t incx, uint64_t y, uint64_t incy) {
    printf("[Grate|stridevec] toy_daxpy handler ran\n");
    fflush(stdout);
    double alpha;
    memcpy(&alpha, &alpha_bits, sizeof(double));
    return LIND_RET_INT(toy_daxpy((int)n, alpha, (const double *)LIND_AS_CPTR(x), (int)incx,
                                   (double *)LIND_AS_PTR(y), (int)incy));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_daxpy, &daxpy_spec, handler_daxpy)

// ---------------------------------------------------------------------------
// toy_daxpy_ref: n/incx/incy by reference. Each stays LIND_ARG_SCALAR in its
// own arg slot (a raw, unmarshalled passthrough of the source-cage pointer
// value) -- X/Y's own extent-operand evaluation is the only place that
// dereferences it for sizing purposes, so a bad n/incx/incy pointer is
// caught there and nowhere else.
// ---------------------------------------------------------------------------
static struct lind_marshal_spec daxpy_ref_spec = {
    .nargs = 6,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n (pointer value)
        { .kind = LIND_ARG_SCALAR },  // alpha
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_POINTEE_I32 },
          .stride_operand = { .arg_index = 3, .source = LIND_EXTENT_POINTEE_I32 },
          .const_size = sizeof(double) },  // x
        { .kind = LIND_ARG_SCALAR },  // incx (pointer value)
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_INOUT,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_POINTEE_I32 },
          .stride_operand = { .arg_index = 5, .source = LIND_EXTENT_POINTEE_I32 },
          .const_size = sizeof(double) },  // y
        { .kind = LIND_ARG_SCALAR },  // incy (pointer value)
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_daxpy_ref(uint64_t n_ptr, uint64_t alpha_bits, uint64_t x,
                                   uint64_t incx_ptr, uint64_t y, uint64_t incy_ptr) {
    printf("[Grate|stridevec] toy_daxpy_ref handler ran\n");
    fflush(stdout);
    double alpha;
    memcpy(&alpha, &alpha_bits, sizeof(double));
    int n_val, incx_val, incy_val;
    _lind_copy_or_abort(LIND_GRATE_CAGE(), LIND_SOURCE_CAGE(), n_ptr, LIND_SOURCE_CAGE(),
                         (uint64_t)(uintptr_t)&n_val, LIND_GRATE_CAGE(), sizeof(int), 0);
    _lind_copy_or_abort(LIND_GRATE_CAGE(), LIND_SOURCE_CAGE(), incx_ptr, LIND_SOURCE_CAGE(),
                         (uint64_t)(uintptr_t)&incx_val, LIND_GRATE_CAGE(), sizeof(int), 0);
    _lind_copy_or_abort(LIND_GRATE_CAGE(), LIND_SOURCE_CAGE(), incy_ptr, LIND_SOURCE_CAGE(),
                         (uint64_t)(uintptr_t)&incy_val, LIND_GRATE_CAGE(), sizeof(int), 0);
    return LIND_RET_INT(toy_daxpy_ref(&n_val, alpha, (const double *)LIND_AS_CPTR(x),
                                       &incx_val, (double *)LIND_AS_PTR(y), &incy_val));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_daxpy_ref, &daxpy_ref_spec, handler_daxpy_ref)

// ---------------------------------------------------------------------------
// toy_daxpy_mixed: n/incy by reference, incx by value.
// ---------------------------------------------------------------------------
static struct lind_marshal_spec daxpy_mixed_spec = {
    .nargs = 6,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n (pointer value)
        { .kind = LIND_ARG_SCALAR },  // alpha
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_POINTEE_I32 },
          .stride_operand = { .arg_index = 3, .source = LIND_EXTENT_VALUE },
          .const_size = sizeof(double) },  // x
        { .kind = LIND_ARG_SCALAR },  // incx (direct value)
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_INOUT,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_POINTEE_I32 },
          .stride_operand = { .arg_index = 5, .source = LIND_EXTENT_POINTEE_I32 },
          .const_size = sizeof(double) },  // y
        { .kind = LIND_ARG_SCALAR },  // incy (pointer value)
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_daxpy_mixed(uint64_t n_ptr, uint64_t alpha_bits, uint64_t x,
                                     uint64_t incx, uint64_t y, uint64_t incy_ptr) {
    printf("[Grate|stridevec] toy_daxpy_mixed handler ran\n");
    fflush(stdout);
    double alpha;
    memcpy(&alpha, &alpha_bits, sizeof(double));
    int n_val, incy_val;
    _lind_copy_or_abort(LIND_GRATE_CAGE(), LIND_SOURCE_CAGE(), n_ptr, LIND_SOURCE_CAGE(),
                         (uint64_t)(uintptr_t)&n_val, LIND_GRATE_CAGE(), sizeof(int), 0);
    _lind_copy_or_abort(LIND_GRATE_CAGE(), LIND_SOURCE_CAGE(), incy_ptr, LIND_SOURCE_CAGE(),
                         (uint64_t)(uintptr_t)&incy_val, LIND_GRATE_CAGE(), sizeof(int), 0);
    return LIND_RET_INT(toy_daxpy_mixed(&n_val, alpha, (const double *)LIND_AS_CPTR(x),
                                         (int)incx, (double *)LIND_AS_PTR(y), &incy_val));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_daxpy_mixed, &daxpy_mixed_spec, handler_daxpy_mixed)

// ---------------------------------------------------------------------------
// toy_daxpy_badindex: correct real function, deliberately corrupt spec (X's
// count operand names an argument slot beyond LIND_RAW_ARGS_MAX).
// ---------------------------------------------------------------------------
static struct lind_marshal_spec daxpy_badindex_spec = {
    .nargs = 6,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // n
        { .kind = LIND_ARG_SCALAR },  // alpha
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 99, .source = LIND_EXTENT_VALUE },  // out of range
          .stride_operand = { .arg_index = 3,  .source = LIND_EXTENT_VALUE },
          .const_size = sizeof(double) },  // x
        { .kind = LIND_ARG_SCALAR },  // incx
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_INOUT,
          .size_kind = LIND_SIZE_STRIDE_VECTOR,
          .size_operand   = { .arg_index = 0, .source = LIND_EXTENT_VALUE },
          .stride_operand = { .arg_index = 5, .source = LIND_EXTENT_VALUE },
          .const_size = sizeof(double) },  // y
        { .kind = LIND_ARG_SCALAR },  // incy
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_daxpy_badindex(uint64_t n, uint64_t alpha_bits, uint64_t x,
                                        uint64_t incx, uint64_t y, uint64_t incy) {
    printf("[Grate|stridevec] toy_daxpy_badindex handler ran (should not happen)\n");
    fflush(stdout);
    double alpha;
    memcpy(&alpha, &alpha_bits, sizeof(double));
    return LIND_RET_INT(toy_daxpy_badindex((int)n, alpha, (const double *)LIND_AS_CPTR(x), (int)incx,
                                            (double *)LIND_AS_PTR(y), (int)incy));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_daxpy_badindex, &daxpy_badindex_spec, handler_daxpy_badindex)

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
        fprintf(stderr, "[Grate|stridevec] invalid fn ptr\n");
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
        ret = register_lib_handler(cageid, "env", "toy_daxpy", grateid, (uint64_t)(uintptr_t)&lind_mh_toy_daxpy);
        if (ret != 0) { fprintf(stderr, "[Grate|stridevec] register toy_daxpy failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_daxpy_ref", grateid, (uint64_t)(uintptr_t)&lind_mh_toy_daxpy_ref);
        if (ret != 0) { fprintf(stderr, "[Grate|stridevec] register toy_daxpy_ref failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_daxpy_mixed", grateid, (uint64_t)(uintptr_t)&lind_mh_toy_daxpy_mixed);
        if (ret != 0) { fprintf(stderr, "[Grate|stridevec] register toy_daxpy_mixed failed\n"); assert(0); }
        ret = register_lib_handler(cageid, "env", "toy_daxpy_badindex", grateid, (uint64_t)(uintptr_t)&lind_mh_toy_daxpy_badindex);
        if (ret != 0) { fprintf(stderr, "[Grate|stridevec] register toy_daxpy_badindex failed\n"); assert(0); }

        printf("[Grate|stridevec] registered 4/4 handlers\n");
        fflush(stdout);
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[Grate|stridevec] app exited %d\n", ce);
    return 0;
}
