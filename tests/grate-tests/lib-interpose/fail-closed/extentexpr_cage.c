// Cage exercising lind_marshal.h's general lind_extent_expr tree (issue
// #22 / OpenBLAS inference integration, Gate 1). See extentexpr_grate.c
// for exactly which tree each mode builds.
//
// Every mode calls toy_extent_probe(mode, a0, a1, a2, a3, a4) and checks
// either the exact returned value (accept modes) or LIND_GRATE_ERR
// (reject modes) -- never just "didn't crash". Every result this
// evaluator can legitimately produce is non-negative, and LIND_GRATE_ERR
// is a small-magnitude negative sentinel, so the two can never collide.
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <limits.h>

#define LIND_GRATE_ERR (-536805379LL)

// a0/a1 are int64_t (not int): the leaf-type tests need to pass values
// that don't fit a 32-bit argument at all (see extentexpr_grate.c's
// LIND_EXTENT_LEAF_I64/_U64 modes) -- a value that DOES fit 32 bits
// behaves identically through the wider parameter, so this doesn't change
// any existing mode's behavior.
extern long long toy_extent_probe(int mode, int64_t a0, int64_t a1, uint32_t a2, int a3, int a4);

static int expect_value(const char *label, long long got, long long want) {
    if (got != want) {
        printf("[Cage|extentexpr] FAIL: %s got=%lld want=%lld\n", label, got, want);
        return 1;
    }
    printf("[Cage|extentexpr] PASS: %s\n", label);
    return 0;
}

static int expect_reject(const char *label, long long got) {
    if (got != LIND_GRATE_ERR) {
        printf("[Cage|extentexpr] FAIL: %s did not reject (got=%lld)\n", label, got);
        return 1;
    }
    printf("[Cage|extentexpr] PASS: %s\n", label);
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: %s <mode>\n", argv[0]); return 2; }
    const char *mode = argv[1];
    long long r;
    int pointee_val = 99;
    uint32_t pointee_addr = (uint32_t)(uintptr_t)&pointee_val;

    if (strcmp(mode, "constant") == 0) {
        r = toy_extent_probe(0, 0, 0, 0, 0, 0);
        return expect_value("constant", r, 42);
    }
    if (strcmp(mode, "arg_value") == 0) {
        r = toy_extent_probe(1, 7, 0, 0, 0, 0);
        return expect_value("arg_value", r, 7);
    }
    if (strcmp(mode, "arg_pointee") == 0) {
        r = toy_extent_probe(2, 0, 0, pointee_addr, 0, 0);
        return expect_value("arg_pointee", r, 99);
    }
    if (strcmp(mode, "abs") == 0) {
        r = toy_extent_probe(3, -5, 0, 0, 0, 0);
        return expect_value("abs", r, 5);
    }
    if (strcmp(mode, "abs-int-min") == 0) {
        r = toy_extent_probe(19, INT32_MIN, 0, 0, 0, 0);
        return expect_value("abs-int-min", r, 2147483648LL);
    }
    if (strcmp(mode, "product") == 0) {
        r = toy_extent_probe(4, 6, 7, 0, 0, 0);
        return expect_value("product", r, 42);
    }
    if (strcmp(mode, "product-overflow") == 0) {
        // Mode 5's tree uses two internal CONSTANT leaves (3037000500 each,
        // just over sqrt(INT64_MAX)) -- a0/a1 are unused.
        r = toy_extent_probe(5, 0, 0, 0, 0, 0);
        return expect_reject("product-overflow", r);
    }
    if (strcmp(mode, "max") == 0) {
        r = toy_extent_probe(6, 3, 9, 0, 0, 0);
        return expect_value("max", r, 9);
    }
    if (strcmp(mode, "add") == 0) {
        r = toy_extent_probe(7, 5, 37, 0, 0, 0);
        return expect_value("add", r, 42);
    }
    if (strcmp(mode, "add-overflow") == 0) {
        r = toy_extent_probe(8, 0, 0, 0, 0, 0);
        return expect_reject("add-overflow", r);
    }
    if (strcmp(mode, "ceildiv-round") == 0) {
        r = toy_extent_probe(9, 0, 0, 0, 0, 0);
        return expect_value("ceildiv-round", r, 4);
    }
    if (strcmp(mode, "ceildiv-exact") == 0) {
        r = toy_extent_probe(10, 0, 0, 0, 0, 0);
        return expect_value("ceildiv-exact", r, 3);
    }
    if (strcmp(mode, "ceildiv-byzero") == 0) {
        r = toy_extent_probe(11, 0, 0, 0, 0, 0);
        return expect_reject("ceildiv-byzero", r);
    }
    if (strcmp(mode, "ceildiv-negdividend") == 0) {
        // A negative dividend taints rather than aborts (see
        // LIND_EXPR_CEIL_DIVIDE's own doc): the real callee's own XERBLA-
        // style validation was always going to reject this request on its
        // own, more gracefully, so this resolves to an empty (0-byte)
        // shadow instead of trapping the whole process pre-emptively.
        r = toy_extent_probe(12, -5, 0, 0, 0, 0);
        return expect_value("ceildiv-negdividend", r, 0);
    }
    if (strcmp(mode, "ceildiv-negdivisor") == 0) {
        r = toy_extent_probe(20, -2, 0, 0, 0, 0);
        return expect_reject("ceildiv-negdivisor", r);
    }
    if (strcmp(mode, "negative-root") == 0) {
        // A directly negative final value resolves to 0, the same
        // reasoning as ceildiv-negdividend above: a negative extent is
        // never a request this marshaller should service with real
        // memory, but the real callee was always going to reject it
        // itself, so there is no reason to abort the whole process first.
        r = toy_extent_probe(13, -1, 0, 0, 0, 0);
        return expect_value("negative-root", r, 0);
    }
    if (strcmp(mode, "zero-root") == 0) {
        r = toy_extent_probe(1, 0, 0, 0, 0, 0);
        return expect_value("zero-root", r, 0);
    }
    if (strcmp(mode, "sizet-overflow") == 0) {
        r = toy_extent_probe(14, 0, 0, 0, 0, 0);
        return expect_reject("sizet-overflow", r);
    }
    if (strcmp(mode, "bad-arg-index") == 0) {
        r = toy_extent_probe(15, 0, 0, 0, 0, 0);
        return expect_reject("bad-arg-index", r);
    }
    if (strcmp(mode, "bad-kind") == 0) {
        r = toy_extent_probe(16, 0, 0, 0, 0, 0);
        return expect_reject("bad-kind", r);
    }
    if (strcmp(mode, "excessive-depth") == 0) {
        r = toy_extent_probe(17, 0, 0, 0, 0, 0);
        return expect_reject("excessive-depth", r);
    }
    if (strcmp(mode, "packed-storage") == 0) {
        r = toy_extent_probe(18, 5, 0, 0, 0, 0);
        return expect_value("packed-storage", r, 15);
    }
    if (strcmp(mode, "packed-storage-zero") == 0) {
        r = toy_extent_probe(18, 0, 0, 0, 0, 0);
        return expect_value("packed-storage-zero", r, 0);
    }
    if (strcmp(mode, "null-child") == 0) {
        r = toy_extent_probe(21, 0, 0, 0, 0, 0);
        return expect_reject("null-child", r);
    }
    if (strcmp(mode, "const-too-big") == 0) {
        r = toy_extent_probe(22, 0, 0, 0, 0, 0);
        return expect_reject("const-too-big", r);
    }
    if (strcmp(mode, "pointee-null") == 0) {
        r = toy_extent_probe(23, 0, 0, 0, 0, 0);
        return expect_reject("pointee-null", r);
    }
    if (strcmp(mode, "pointee-invalid") == 0) {
        // a0 = 2^32: not a valid wasm32 address (exceeds UINT32_MAX), but
        // representable in a0's own int64_t width -- exactly the case
        // arg2's uint32_t-typed parameter couldn't construct.
        r = toy_extent_probe(23, 4294967296LL, 0, 0, 0, 0);
        return expect_reject("pointee-invalid", r);
    }
    if (strcmp(mode, "leaf-u32") == 0) {
        // -1 read as U32 (zero-extended): 4294967295, not -1.
        r = toy_extent_probe(24, -1, 0, 0, 0, 0);
        return expect_value("leaf-u32", r, 4294967295LL);
    }
    if (strcmp(mode, "leaf-i64") == 0) {
        // 3000000000 exceeds INT32_MAX (misread as i32 it would even come
        // back NEGATIVE: bit 31 is set), but still fits a wasm32 size_t
        // (unlike 5000000000, which would fail the FINAL size_t check
        // regardless of whether the leaf itself were read correctly).
        r = toy_extent_probe(25, 3000000000LL, 0, 0, 0, 0);
        return expect_value("leaf-i64", r, 3000000000LL);
    }
    if (strcmp(mode, "leaf-u64") == 0) {
        r = toy_extent_probe(26, 3000000000LL, 0, 0, 0, 0);
        return expect_value("leaf-u64", r, 3000000000LL);
    }
    if (strcmp(mode, "leaf-u64-overflow") == 0) {
        // -1 as u64 is UINT64_MAX, which doesn't fit int64_t's
        // representable (non-negative) range at all.
        r = toy_extent_probe(26, -1, 0, 0, 0, 0);
        return expect_reject("leaf-u64-overflow", r);
    }
    if (strcmp(mode, "product-negative-operands") == 0) {
        // product(-2, -3): both operands negative must resolve to 0, not
        // silently succeed as the coincidentally-positive 6 the raw
        // multiplication computes -- the taint flag catches this
        // specifically because it tracks every operand independently,
        // not just the tree's own final numeric value (which alone
        // cannot tell this case apart from a genuinely valid product(2,3)).
        r = toy_extent_probe(27, -2, -3, 0, 0, 0);
        return expect_value("product-negative-operands", r, 0);
    }
    if (strcmp(mode, "node-budget-exceeded") == 0) {
        r = toy_extent_probe(28, 0, 0, 0, 0, 0);
        return expect_reject("node-budget-exceeded", r);
    }

    fprintf(stderr, "unknown mode: %s\n", mode);
    return 2;
}
