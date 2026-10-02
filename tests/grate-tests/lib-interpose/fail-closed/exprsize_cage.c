// Cage exercising lind_marshal.h's LIND_SIZE_EXPR and
// LIND_SIZE_STRIDE_VECTOR's general lind_extent_expr operands (issue #22's
// OpenBLAS inference-to-runtime integration, Gate 3). See exprsize_grate.c
// for the real functions/specs each mode targets.
//
// Usage: <mode>
//   basic      -- toy_daxpy_exprstride, n/incx/incy all positive: checks
//                  the EXACT expected values at the touched positions,
//                  proving the tree-shaped operands size/compute the same
//                  as stridevec_grate.c's plain-operand daxpy test does.
//   negstride  -- toy_daxpy_exprstride with a NEGATIVE incx: unlike
//                  stridevec's own "negstride" mode (rejected outright --
//                  a plain lind_extent_operand leaf has no way to express
//                  "take the magnitude"), this must be ACCEPTED: the
//                  stride operand is abs()-wrapped, matching real OpenBLAS
//                  inference's own formula shape for cblas_dger/
//                  cblas_dnrm2/etc.
//   packedsum  -- toy_packedsum_exprsize: checks the EXACT expected sum of
//                  a packed n*(n+1)/2-element buffer sized entirely by a
//                  LIND_SIZE_EXPR tree.
//   badtree    -- toy_packedsum_badtree, whose spec's size_expr tree names
//                  an out-of-range argument index: must be rejected.
#include <stdio.h>
#include <string.h>
#include <stdint.h>

#define LIND_GRATE_ERR (-536805379L)

extern int toy_daxpy_exprstride(int n, double alpha, const double *x, int incx,
                                 double *y, int incy);
extern double toy_packedsum_exprsize(int n, const double *ap);
extern int toy_packedsum_badtree(int n, const double *ap);

static int check_y(const char *label, const double *y, const double *expected, int len) {
    for (int i = 0; i < len; i++) {
        if (y[i] != expected[i]) {
            printf("[Cage|exprsize] FAIL: %s y[%d]=%g expected %g\n", label, i, y[i], expected[i]);
            return 1;
        }
    }
    printf("[Cage|exprsize] PASS: %s\n", label);
    return 0;
}

static int run_basic(void) {
    const int n = 4, incx = 2, incy = 3;
    double alpha = 2.0;
    double x[7] = { 1, 2, 3, 4, 5, 6, 7 };   // span = 1+(4-1)*2 = 7
    double y[10] = { 0 };                     // span = 1+(4-1)*3 = 10

    long r = toy_daxpy_exprstride(n, alpha, x, incx, y, incy);
    if (r == LIND_GRATE_ERR) { printf("[Cage|exprsize] FAIL: basic wrongly rejected\n"); return 1; }

    double expected[10] = { 0 };
    expected[0] = alpha * x[0];  // 2
    expected[3] = alpha * x[2];  // 6
    expected[6] = alpha * x[4];  // 10
    expected[9] = alpha * x[6];  // 14
    return check_y("basic", y, expected, 10);
}

static int run_negstride(void) {
    const int n = 4, incx = -2, incy = 1;
    double alpha = 2.0;
    double x[7] = { 1, 2, 3, 4, 5, 6, 7 };   // span = 1+(4-1)*abs(-2) = 7
    double y[4] = { 0 };                      // span = 1+(4-1)*1 = 4

    long r = toy_daxpy_exprstride(n, alpha, x, incx, y, incy);
    if (r == LIND_GRATE_ERR) { printf("[Cage|exprsize] FAIL: negstride wrongly rejected\n"); return 1; }

    double expected[4] = { alpha * x[0], alpha * x[2], alpha * x[4], alpha * x[6] };
    return check_y("negstride", y, expected, 4);
}

static int run_packedsum(void) {
    const int n = 4;  // count = 4*5/2 = 10
    double ap[10] = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 };
    double want = 55.0;

    double got = toy_packedsum_exprsize(n, ap);
    if (got != want) {
        printf("[Cage|exprsize] FAIL: packedsum got=%g want=%g\n", got, want);
        return 1;
    }
    printf("[Cage|exprsize] PASS: packedsum\n");
    return 0;
}

static int run_badtree(void) {
    double ap[10] = { 0 };
    long r = toy_packedsum_badtree(4, ap);
    if (r != LIND_GRATE_ERR) {
        printf("[Cage|exprsize] FAIL: badtree did not reject (r=%ld)\n", r);
        return 1;
    }
    printf("[Cage|exprsize] PASS: badtree\n");
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: %s <mode>\n", argv[0]); return 2; }
    const char *mode = argv[1];
    if (strcmp(mode, "basic") == 0) return run_basic();
    if (strcmp(mode, "negstride") == 0) return run_negstride();
    if (strcmp(mode, "packedsum") == 0) return run_packedsum();
    if (strcmp(mode, "badtree") == 0) return run_badtree();
    fprintf(stderr, "unknown mode: %s\n", mode);
    return 2;
}
