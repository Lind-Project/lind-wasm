/* A plain native program that uses BLAS through the standard cblas names, unaware
 * the work runs inside the wasm sandbox. It links our native libopenblas.so (the
 * stub cdylib) exactly as it would link the real one — a binary drop-in.
 *
 * Self-checking: expected results are hand-computed, so it needs no native OpenBLAS
 * baseline to validate. */
#include <stdio.h>
#include <stddef.h>
#include <math.h>

/* Standard CBLAS prototypes (subset). Matches <cblas.h>. */
size_t cblas_idamax(const int n, const double *x, const int incx);
double cblas_ddot(const int n, const double *x, const int incx,
                  const double *y, const int incy);
void   cblas_daxpy(const int n, const double alpha, const double *x, const int incx,
                   double *y, const int incy);

static int check_d(const char *what, double got, double want) {
    int ok = fabs(got - want) < 1e-9;
    printf("%-14s -> %g (want %g) %s\n", what, got, want, ok ? "OK" : "FAIL");
    return ok;
}

int main(void) {
    double x[4] = { 1.0, 2.0, 3.0, 4.0 };
    double y[4] = { 10.0, 20.0, 30.0, 40.0 };
    int fails = 0;

    /* idamax: index of max |x| = 3 (the 4.0). */
    size_t idx = cblas_idamax(4, x, 1);
    printf("%-14s -> %zu (want 3) %s\n", "cblas_idamax", idx, idx == 3 ? "OK" : "FAIL");
    fails += (idx != 3);

    /* ddot: 1*10 + 2*20 + 3*30 + 4*40 = 300. */
    fails += !check_d("cblas_ddot", cblas_ddot(4, x, 1, y, 1), 300.0);

    /* daxpy: y := 2*x + y = {12, 24, 36, 48}. */
    cblas_daxpy(4, 2.0, x, 1, y, 1);
    double sum = y[0] + y[1] + y[2] + y[3];               /* 12+24+36+48 = 120 */
    fails += !check_d("cblas_daxpy", sum, 120.0);
    printf("               y = {%g, %g, %g, %g}\n", y[0], y[1], y[2], y[3]);

    printf(fails ? "\n%d check(s) FAILED\n" : "\nall checks passed\n", fails);
    return fails ? 1 : 0;
}
