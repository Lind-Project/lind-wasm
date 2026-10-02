// Grate exercising lind_marshal.h's general lind_extent_expr tree and its
// evaluator, _lind_eval_extent_expr (issue #22 / OpenBLAS inference
// integration, Gate 1: "define one runtime extent-expression contract").
//
// Unlike LIND_SIZE_STRIDE_VECTOR's own lind_extent_operand (a single leaf,
// exercised by stridevec_grate.c/stridevec_cage.c), this tests the general
// tree directly: the handler below builds a different hand-written
// lind_extent_expr per `mode` and evaluates it against the call's own
// scalar arguments (reconstructed into a local raw_args[] array, since a
// V1 handler only receives individual scalars, not the dispatcher's own
// internal array) -- proving every node kind, nested combinations, and
// every fail-closed edge case named by the gate: zero, negative inputs,
// INT_MIN, divide-by-zero, intermediate overflow, final size_t overflow,
// excessive depth, invalid tags, and invalid argument indices.
//
// toy_extent_probe's own spec is trivial (six plain scalars, one scalar
// return) specifically so it carries no marshalling of its own to
// interfere with what's under test: every tree built here evaluates
// purely against toy_extent_probe's own raw argument values, not against
// anything lind_marshal_dispatch itself already validated.
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>
#include <limits.h>

#include "../lind_marshal.h"

static struct lind_marshal_spec probe_spec = {
    .nargs = 6,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // mode
        { .kind = LIND_ARG_SCALAR },  // a0
        { .kind = LIND_ARG_SCALAR },  // a1
        { .kind = LIND_ARG_SCALAR },  // a2 (pointer value, for the pointee mode)
        { .kind = LIND_ARG_SCALAR },  // a3
        { .kind = LIND_ARG_SCALAR },  // a4
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

// Every mode's tree nodes, built once as static storage (lind_extent_expr
// nodes are referenced by pointer, matching lind_arg_spec/lind_layout's
// own convention elsewhere in this file).
static const struct lind_extent_expr E_CONST_42        = { .kind = LIND_EXPR_CONSTANT, .const_value = 42 };
static const struct lind_extent_expr E_ARG0            = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0 };
static const struct lind_extent_expr E_ARG1            = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 1 };
static const struct lind_extent_expr E_ARG2_POINTEE    = { .kind = LIND_EXPR_ARG_POINTEE_I32, .arg_index = 2 };
static const struct lind_extent_expr E_ARG_BADINDEX    = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 99 };
static const struct lind_extent_expr E_BAD_KIND        = { .kind = (enum lind_extent_expr_kind)999 };

static const struct lind_extent_expr E_ABS_ARG0        = { .kind = LIND_EXPR_ABS, .lhs = &E_ARG0 };

static const struct lind_extent_expr E_PRODUCT_ARG0_ARG1 = { .kind = LIND_EXPR_PRODUCT, .lhs = &E_ARG0, .rhs = &E_ARG1 };
// Overflow needs CONSTANT leaves, not ARG_VALUE ones: an ARG_VALUE leaf is
// sign-extended as a 32-bit int (real C `int` semantics), so no two
// ARG_VALUE leaves can ever multiply past INT64_MAX in the first place
// (|a|*|b| <= (2^31)^2 == 2^62 < INT64_MAX's 2^63-1). CONSTANT is u64, with
// no such cap -- 3037000500^2 is just over INT64_MAX.
static const struct lind_extent_expr E_CONST_BIGSQRT   = { .kind = LIND_EXPR_CONSTANT, .const_value = 3037000500ULL };
static const struct lind_extent_expr E_PRODUCT_OVERFLOW = { .kind = LIND_EXPR_PRODUCT, .lhs = &E_CONST_BIGSQRT, .rhs = &E_CONST_BIGSQRT };

static const struct lind_extent_expr E_MAX_ARG0_ARG1   = { .kind = LIND_EXPR_MAX, .lhs = &E_ARG0, .rhs = &E_ARG1 };

static const struct lind_extent_expr E_ADD_ARG0_ARG1   = { .kind = LIND_EXPR_ADD, .lhs = &E_ARG0, .rhs = &E_ARG1 };
// Overflow: two near-INT64_MAX constants (each individually representable,
// i.e. <= INT64_MAX, but their sum isn't).
static const struct lind_extent_expr E_ADD_CONST_BIG_A = { .kind = LIND_EXPR_CONSTANT, .const_value = 9223372036854775800ULL };
static const struct lind_extent_expr E_ADD_CONST_BIG_B = { .kind = LIND_EXPR_CONSTANT, .const_value = 100 };
static const struct lind_extent_expr E_ADD_OVERFLOW    = { .kind = LIND_EXPR_ADD, .lhs = &E_ADD_CONST_BIG_A, .rhs = &E_ADD_CONST_BIG_B };

static const struct lind_extent_expr E_CONST_7         = { .kind = LIND_EXPR_CONSTANT, .const_value = 7 };
static const struct lind_extent_expr E_CONST_6         = { .kind = LIND_EXPR_CONSTANT, .const_value = 6 };
static const struct lind_extent_expr E_CONST_2         = { .kind = LIND_EXPR_CONSTANT, .const_value = 2 };
static const struct lind_extent_expr E_CONST_0         = { .kind = LIND_EXPR_CONSTANT, .const_value = 0 };
static const struct lind_extent_expr E_CEILDIV_7_2     = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &E_CONST_7, .rhs = &E_CONST_2 };
static const struct lind_extent_expr E_CEILDIV_6_2     = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &E_CONST_6, .rhs = &E_CONST_2 };
static const struct lind_extent_expr E_CEILDIV_BYZERO  = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &E_CONST_7, .rhs = &E_CONST_0 };
static const struct lind_extent_expr E_CEILDIV_NEGDIV  = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &E_ARG0, .rhs = &E_CONST_2 };

static const struct lind_extent_expr E_NEG_ROOT        = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0 };

// A value that fits int64_t comfortably but exceeds a wasm32 size_t
// (2^32): 0x200000000 == 8589934592.
static const struct lind_extent_expr E_SIZE_T_OVERFLOW = { .kind = LIND_EXPR_CONSTANT, .const_value = 8589934592ULL };

// A NULL operator child -- distinct from E_BAD_KIND (an unrecognized tag)
// and E_ARG_BADINDEX (a well-formed leaf with an out-of-range index): this
// is a well-formed operator node whose required child pointer is simply
// absent, caught by _lind_eval_extent_expr_depth's own entry check.
static const struct lind_extent_expr E_NULL_CHILD      = { .kind = LIND_EXPR_ABS, .lhs = NULL };

// A constant that doesn't fit int64_t at all (distinct from
// E_SIZE_T_OVERFLOW, which IS a valid int64_t, just too big for a wasm32
// size_t) -- caught by LIND_EXPR_CONSTANT's own representable-range check
// before anything about size_t is even considered.
static const struct lind_extent_expr E_CONST_TOO_BIG   = { .kind = LIND_EXPR_CONSTANT, .const_value = 0xFFFFFFFFFFFFFFFFULL };

// Pointee leaf sourced from arg_index 0 (a0, widened to int64_t in the
// cage -- see handler_probe's own doc) rather than arg_index 2: lets the
// "pointee reads an out-of-range address" case below pass a value that
// doesn't fit uint32_t at all, which arg2's own uint32_t-typed cage
// parameter cannot represent.
static const struct lind_extent_expr E_ARG0_POINTEE    = { .kind = LIND_EXPR_ARG_POINTEE_I32, .arg_index = 0 };

// Leaf-type (ABI width/signedness) coverage: the SAME raw bit pattern
// (cage passes a0 = -1, i.e. every bit set) means something different --
// and is valid or invalid differently -- depending on how the leaf is
// declared. LIND_EXTENT_LEAF_I32 is the implicit default (field left
// unset); the other three are explicit here.
static const struct lind_extent_expr E_ARG0_U32        = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0, .leaf_type = LIND_EXTENT_LEAF_U32 };
static const struct lind_extent_expr E_ARG0_I64        = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0, .leaf_type = LIND_EXTENT_LEAF_I64 };
static const struct lind_extent_expr E_ARG0_U64        = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0, .leaf_type = LIND_EXTENT_LEAF_U64 };

static uint64_t handler_probe(uint64_t mode, uint64_t a0, uint64_t a1,
                               uint64_t a2, uint64_t a3, uint64_t a4) {
    printf("[Grate|extentexpr] toy_extent_probe handler ran mode=%llu\n",
           (unsigned long long)mode);
    fflush(stdout);
    // mode is a dispatch selector for THIS TEST, not part of the
    // expression's own argument space -- every tree below uses arg_index
    // 0..4 to mean a0..a4, so mode must not occupy slot 0.
    uint64_t raw_args[5] = { a0, a1, a2, a3, a4 };
    uint32_t nargs = 5;
    uint64_t sc = LIND_SOURCE_CAGE(), gc = LIND_GRATE_CAGE();
    const struct lind_extent_expr *e = NULL;

    switch (mode) {
        case 0:  e = &E_CONST_42; break;
        case 1:  e = &E_ARG0; break;              // plain arg_value
        case 2:  e = &E_ARG2_POINTEE; break;      // arg_pointee_i32
        case 3:  e = &E_ABS_ARG0; break;          // abs (also used for INT_MIN)
        case 4:  e = &E_PRODUCT_ARG0_ARG1; break; // product
        case 5:  e = &E_PRODUCT_OVERFLOW; break;  // product overflow
        case 6:  e = &E_MAX_ARG0_ARG1; break;     // max
        case 7:  e = &E_ADD_ARG0_ARG1; break;     // add
        case 8:  e = &E_ADD_OVERFLOW; break;      // add overflow
        case 9:  e = &E_CEILDIV_7_2; break;       // ceil_divide, rounds up
        case 10: e = &E_CEILDIV_6_2; break;       // ceil_divide, exact
        case 11: e = &E_CEILDIV_BYZERO; break;    // ceil_divide by zero
        case 12: e = &E_CEILDIV_NEGDIV; break;    // ceil_divide of a negative dividend (a0)
        case 13: e = &E_NEG_ROOT; break;          // negative final extent (a0)
        case 14: e = &E_SIZE_T_OVERFLOW; break;   // final size_t overflow
        case 15: e = &E_ARG_BADINDEX; break;      // invalid argument index
        case 16: e = &E_BAD_KIND; break;          // invalid/unrecognized node kind
        case 17: {
            // Excessive depth: a right-leaning chain of ADD(CONST_1, ...)
            // LIND_EXTENT_EXPR_MAX_DEPTH+4 levels deep, built on the stack
            // here (fine -- evaluation happens before this frame returns).
            struct lind_extent_expr one = { .kind = LIND_EXPR_CONSTANT, .const_value = 1 };
            struct lind_extent_expr chain[LIND_EXTENT_EXPR_MAX_DEPTH + 4];
            chain[0] = one;
            for (int i = 1; i < LIND_EXTENT_EXPR_MAX_DEPTH + 4; i++) {
                chain[i].kind = LIND_EXPR_ADD;
                chain[i].lhs = &one;
                chain[i].rhs = &chain[i - 1];
            }
            size_t r = _lind_eval_extent_expr(&chain[LIND_EXTENT_EXPR_MAX_DEPTH + 3],
                                               raw_args, nargs, sc, gc);
            return LIND_RET_INT((int64_t)r);
        }
        case 18: {
            // The real packed-storage formula: ceil_divide(product(n,
            // add(n, 1)), 2) with n = a0, built fresh here (not as static
            // storage) since it genuinely needs a0 in two places.
            struct lind_extent_expr arg0 = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0 };
            struct lind_extent_expr one  = { .kind = LIND_EXPR_CONSTANT, .const_value = 1 };
            struct lind_extent_expr n_plus_1 = { .kind = LIND_EXPR_ADD, .lhs = &arg0, .rhs = &one };
            struct lind_extent_expr product = { .kind = LIND_EXPR_PRODUCT, .lhs = &arg0, .rhs = &n_plus_1 };
            struct lind_extent_expr two = { .kind = LIND_EXPR_CONSTANT, .const_value = 2 };
            struct lind_extent_expr packed = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &product, .rhs = &two };
            size_t r = _lind_eval_extent_expr(&packed, raw_args, nargs, sc, gc);
            return LIND_RET_INT((int64_t)r);
        }
        case 19: {
            // abs(INT_MIN): proves the widened (int64_t) representation
            // doesn't overflow where a plain int32_t negation would.
            struct lind_extent_expr arg0 = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0 };
            struct lind_extent_expr absv = { .kind = LIND_EXPR_ABS, .lhs = &arg0 };
            size_t r = _lind_eval_extent_expr(&absv, raw_args, nargs, sc, gc);
            return LIND_RET_INT((int64_t)r);
        }
        case 20: {
            // ceil_divide with a NEGATIVE divisor (not just zero).
            struct lind_extent_expr negdivisor = { .kind = LIND_EXPR_ARG_VALUE, .arg_index = 0 };
            struct lind_extent_expr dividend = { .kind = LIND_EXPR_CONSTANT, .const_value = 7 };
            struct lind_extent_expr div = { .kind = LIND_EXPR_CEIL_DIVIDE, .lhs = &dividend, .rhs = &negdivisor };
            size_t r = _lind_eval_extent_expr(&div, raw_args, nargs, sc, gc);
            return LIND_RET_INT((int64_t)r);
        }
        case 21: e = &E_NULL_CHILD; break;     // null operator child
        case 22: e = &E_CONST_TOO_BIG; break;  // constant doesn't fit int64_t at all
        case 23: e = &E_ARG0_POINTEE; break;   // pointee: null or out-of-range address (via a0)
        case 24: e = &E_ARG0_U32; break;       // leaf_type U32
        case 25: e = &E_ARG0_I64; break;       // leaf_type I64
        case 26: e = &E_ARG0_U64; break;       // leaf_type U64 (success and overflow-reject)
        case 27: e = &E_PRODUCT_ARG0_ARG1; break; // product(-2,-3): both operands negative, must reject
        case 28: {
            // Node-budget-exceeded: a BALANCED tree of ADD(CONST_1, ...)
            // nodes -- depth only log2(leaves), well under
            // LIND_EXTENT_EXPR_MAX_DEPTH, but total node count well over
            // LIND_EXTENT_EXPR_MAX_NODES. Proves the node-count bound
            // catches what the depth bound alone cannot (a WIDE tree, not
            // a deep one) -- see LIND_EXTENT_EXPR_MAX_NODES's own doc.
            enum { LEAVES = 128 };  // depth = log2(128) = 7; total nodes = 2*LEAVES-1 = 255
            struct lind_extent_expr one = { .kind = LIND_EXPR_CONSTANT, .const_value = 1 };
            struct lind_extent_expr level[LEAVES];
            for (int i = 0; i < LEAVES; i++) level[i] = one;
            int count = LEAVES;
            struct lind_extent_expr *cur = level;
            while (count > 1) {
                struct lind_extent_expr *next = (struct lind_extent_expr *)
                    _lind_marshal_alloc(sizeof(struct lind_extent_expr) * (count / 2));
                for (int i = 0; i < count / 2; i++) {
                    next[i].kind = LIND_EXPR_ADD;
                    next[i].lhs = &cur[2 * i];
                    next[i].rhs = &cur[2 * i + 1];
                }
                cur = next;
                count /= 2;
            }
            size_t r = _lind_eval_extent_expr(cur, raw_args, nargs, sc, gc);
            return LIND_RET_INT((int64_t)r);
        }
        default:
            fprintf(stderr, "[Grate|extentexpr] FAIL: unknown mode %llu\n", (unsigned long long)mode);
            assert(0);
    }

    size_t result = _lind_eval_extent_expr(e, raw_args, nargs, sc, gc);
    return LIND_RET_INT((int64_t)result);
}
LIND_DEFINE_MARSHAL_HANDLER(toy_extent_probe, &probe_spec, handler_probe)

int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|extentexpr] invalid fn ptr\n");
        assert(0);
    }
    int64_t (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int64_t (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
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
        int ret = register_lib_handler(cageid, "env", "toy_extent_probe", grateid,
                                        (uint64_t)(uintptr_t)&lind_mh_toy_extent_probe);
        if (ret != 0) { fprintf(stderr, "[Grate|extentexpr] register toy_extent_probe failed\n"); assert(0); }
        printf("[Grate|extentexpr] registered 1/1 handlers\n");
        fflush(stdout);
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }
    int status;
    while (wait(&status) > 0) {}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[Grate|extentexpr] app exited %d\n", ce);
    return 0;
}
