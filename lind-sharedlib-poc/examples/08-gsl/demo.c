/* Unmodified consumer: links libgsl.so (our stub) and calls gsl_poly_solve_quadratic /
 * gsl_poly_solve_cubic with no idea the math runs inside the lind sandbox. */
#include <stdio.h>
#include <stddef.h>
#include <stdlib.h>

int gsl_poly_solve_quadratic(double a, double b, double c, double *x0, double *x1);
int gsl_poly_solve_cubic(double a, double b, double c, double *x0, double *x1, double *x2);
int gsl_poly_eval_derivs(const double c[], unsigned long lenc, double x,
                         double res[], unsigned long lenres);
/* sort + statistics: demo links ONLY our stub .so (no native archive), so these MUST
 * resolve to the sandboxed wrappers — a clean check independent of GSL's own test coverage. */
void   gsl_sort(double *data, size_t stride, size_t n);
double gsl_stats_mean(const double data[], size_t stride, size_t n);
/* opaque handle: gsl_vector is never dereferenced here — we hold the stub's token as a
 * void* and only ever pass it to wrapped functions. (Declaring it void* also keeps the
 * GSL header, and its inline accessors, out of this translation unit entirely.) */
void   *gsl_vector_alloc(size_t n);
void    gsl_vector_set(void *v, size_t i, double x);
double  gsl_vector_get(const void *v, size_t i);
void    gsl_sort_vector(void *v);
void    gsl_vector_free(void *v);
/* scalar modules (generated): cdf / sys / specfunc — pure scalar in, scalar out */
double gsl_cdf_ugaussian_P(double x);
double gsl_hypot(double x, double y);
double gsl_sf_bessel_J0(double x);
double gsl_sf_erf(double x);
double gsl_sf_gamma(double x);
/* special-function `_e` form: writes {val,err} into a 16-byte result struct, returns status */
struct demo_sf_result { double val; double err; };
int gsl_sf_bessel_J0_e(double x, struct demo_sf_result *result);
/* Tier-1 tail (hand-written): polynomial divided differences */
int    gsl_poly_dd_init(double dd[], const double xa[], const double ya[], size_t size);
double gsl_poly_dd_eval(const double dd[], const double xa[], size_t size, double x);
/* Tier-2 opaque handle (generated): gsl_matrix — second handle class, held as an opaque token */
void  *gsl_matrix_alloc(size_t n1, size_t n2);
void   gsl_matrix_set(void *m, size_t i, size_t j, double x);
double gsl_matrix_get(const void *m, size_t i, size_t j);
int    gsl_matrix_transpose(void *m);
void   gsl_matrix_free(void *m);
/* Tier-2 opaque handle (generated): gsl_permutation — a third handle class */
void   *gsl_permutation_alloc(size_t n);
void    gsl_permutation_init(void *p);
int     gsl_permutation_swap(void *p, size_t i, size_t j);
size_t  gsl_permutation_get(const void *p, size_t i);
void    gsl_permutation_free(void *p);
/* Tier-2 element-typed handle (generated): gsl_matrix_int — int elements, its own class */
void  *gsl_matrix_int_alloc(size_t n1, size_t n2);
void   gsl_matrix_int_set(void *m, size_t i, size_t j, int x);
int    gsl_matrix_int_get(const void *m, size_t i, size_t j);
void   gsl_matrix_int_free(void *m);
/* Tier-2 cross-class free functions (generated): gsl_linalg LU over matrix+permutation+vector.
 * (gsl_vector_alloc/set/get/free already declared above with the committed handle slice.) */
int    gsl_linalg_LU_decomp(void *A, void *p, int *signum);
int    gsl_linalg_LU_solve(const void *LU, const void *p, const void *b, void *x);
/* Tier-2 handle (generated): gsl_histogram — single class, arrays + double outputs */
void  *gsl_histogram_alloc(size_t n);
int    gsl_histogram_set_ranges_uniform(void *h, double xmin, double xmax);
int    gsl_histogram_increment(void *h, double x);
double gsl_histogram_get(const void *h, size_t i);
void   gsl_histogram_free(void *h);
/* Tier-2 workspace + solver (generated): gsl_eigen — workspace handle + matrix+vector+workspace fn */
void  *gsl_eigen_symm_alloc(size_t n);
int    gsl_eigen_symm(void *A, void *eval, void *w);
void   gsl_eigen_symm_free(void *w);
/* Tier-2 linalg QR (generated, free fns) */
int    gsl_linalg_QR_decomp(void *A, void *tau);
int    gsl_linalg_QR_solve(const void *QR, const void *tau, const void *b, void *x);

int main(void) {
    double x0, x1, x2;

    /* x^2 - 5x + 6 = 0  ->  roots 2, 3 */
    int n = gsl_poly_solve_quadratic(1.0, -5.0, 6.0, &x0, &x1);
    printf("quadratic x^2-5x+6: %d real roots: %g, %g   (expect 2: 2, 3)\n", n, x0, x1);

    /* x^3 - 6x^2 + 11x - 6 = 0  ->  roots 1, 2, 3 */
    n = gsl_poly_solve_cubic(-6.0, 11.0, -6.0, &x0, &x1, &x2);
    printf("cubic x^3-6x^2+11x-6: %d real roots: %g, %g, %g   (expect 3: 1, 2, 3)\n", n, x0, x1, x2);

    /* P(x) = 1 + 2x + 3x^2  at x = 2  ->  P=17, P'=2+6x=14, P''=6.  (input + output arrays) */
    double coeffs[3] = {1.0, 2.0, 3.0};
    double res[3];
    gsl_poly_eval_derivs(coeffs, 3, 2.0, res, 3);
    printf("eval_derivs 1+2x+3x^2 @2: P=%g, P'=%g, P''=%g   (expect 17, 14, 6)\n",
           res[0], res[1], res[2]);

    /* sort: sorts the array in place (InOut) */
    double d[5] = {3.1, 1.2, 4.8, 1.5, 2.6};
    gsl_sort(d, 1, 5);
    printf("gsl_sort -> %g %g %g %g %g   (expect 1.2 1.5 2.6 3.1 4.8)\n",
           d[0], d[1], d[2], d[3], d[4]);

    /* statistics: reduction array -> scalar */
    double s[5] = {2.0, 4.0, 4.0, 4.0, 6.0};
    printf("gsl_stats_mean -> %g   (expect 4)\n", gsl_stats_mean(s, 1, 5));

    /* scalar modules (generated): cdf + sys + specfunc */
    printf("gsl_cdf_ugaussian_P(0) -> %g   (expect 0.5)\n", gsl_cdf_ugaussian_P(0.0));
    printf("gsl_hypot(3,4) -> %g   (expect 5)\n", gsl_hypot(3.0, 4.0));
    printf("gsl_sf_bessel_J0(0) -> %g   (expect 1)\n", gsl_sf_bessel_J0(0.0));
    printf("gsl_sf_erf(0) -> %g   (expect 0)\n", gsl_sf_erf(0.0));
    printf("gsl_sf_gamma(5) -> %g   (expect 24)\n", gsl_sf_gamma(5.0));
    struct demo_sf_result sr = {0};
    int sr_st = gsl_sf_bessel_J0_e(0.0, &sr);
    printf("gsl_sf_bessel_J0_e(0) -> status %d, val %g   (expect 0, 1)\n", sr_st, sr.val);

    /* Tier-1 tail: Newton divided differences for y=x^2 through (0,0),(1,1),(2,4), eval @1.5 */
    double dd_xa[3] = {0.0, 1.0, 2.0}, dd_ya[3] = {0.0, 1.0, 4.0}, dd_c[3];
    gsl_poly_dd_init(dd_c, dd_xa, dd_ya, 3);
    printf("gsl_poly_dd_eval @1.5 -> %g   (expect 2.25)\n", gsl_poly_dd_eval(dd_c, dd_xa, 3, 1.5));

    /* Tier-2 handle: gsl_matrix alloc -> set -> transpose -> get -> free (second handle class) */
    void *mat = gsl_matrix_alloc(2, 2);
    gsl_matrix_set(mat, 0, 0, 1.0); gsl_matrix_set(mat, 0, 1, 2.0);
    gsl_matrix_set(mat, 1, 0, 3.0); gsl_matrix_set(mat, 1, 1, 4.0);
    gsl_matrix_transpose(mat);
    printf("gsl_matrix transpose [0,1]=%g [1,0]=%g   (expect 3, 2)\n",
           gsl_matrix_get(mat, 0, 1), gsl_matrix_get(mat, 1, 0));
    gsl_matrix_free(mat);

    /* Tier-2 handle: gsl_permutation init -> swap -> get -> free */
    void *perm = gsl_permutation_alloc(4);
    gsl_permutation_init(perm);           /* identity: 0 1 2 3 */
    gsl_permutation_swap(perm, 0, 3);     /* -> 3 1 2 0 */
    printf("gsl_permutation swap(0,3): [0]=%zu [3]=%zu   (expect 3, 0)\n",
           gsl_permutation_get(perm, 0), gsl_permutation_get(perm, 3));
    gsl_permutation_free(perm);

    /* Tier-2 element-typed handle: gsl_matrix_int (int elements) */
    void *mi = gsl_matrix_int_alloc(2, 2);
    gsl_matrix_int_set(mi, 0, 0, 7);
    printf("gsl_matrix_int_get(0,0) -> %d   (expect 7)\n", gsl_matrix_int_get(mi, 0, 0));
    gsl_matrix_int_free(mi);

    /* Tier-2 cross-class: solve A x = b with LU over matrix + permutation + 2 vectors.
     * A=[[2,1],[1,3]], b=[3,5] -> x=[0.8,1.4]. Four handle classes in one flow. */
    void *A = gsl_matrix_alloc(2, 2);
    gsl_matrix_set(A, 0, 0, 2); gsl_matrix_set(A, 0, 1, 1);
    gsl_matrix_set(A, 1, 0, 1); gsl_matrix_set(A, 1, 1, 3);
    void *bvec = gsl_vector_alloc(2); gsl_vector_set(bvec, 0, 3); gsl_vector_set(bvec, 1, 5);
    void *xvec = gsl_vector_alloc(2);
    void *lup = gsl_permutation_alloc(2);
    int signum;
    gsl_linalg_LU_decomp(A, lup, &signum);
    gsl_linalg_LU_solve(A, lup, bvec, xvec);
    printf("gsl_linalg_LU_solve -> x0=%g x1=%g   (expect 0.8, 1.4)\n",
           gsl_vector_get(xvec, 0), gsl_vector_get(xvec, 1));
    gsl_matrix_free(A); gsl_vector_free(bvec); gsl_vector_free(xvec); gsl_permutation_free(lup);

    /* Tier-2 handle: gsl_histogram over [0,4) in 4 bins; count values into bins */
    void *hist = gsl_histogram_alloc(4);
    gsl_histogram_set_ranges_uniform(hist, 0.0, 4.0);
    gsl_histogram_increment(hist, 1.5);
    gsl_histogram_increment(hist, 1.2);
    gsl_histogram_increment(hist, 2.5);
    printf("gsl_histogram bin1=%g bin2=%g   (expect 2, 1)\n",
           gsl_histogram_get(hist, 1), gsl_histogram_get(hist, 2));
    gsl_histogram_free(hist);

    /* Tier-2 workspace + solver: eigenvalues of symmetric [[2,1],[1,2]] are 1 and 3. */
    void *S = gsl_matrix_alloc(2, 2);
    gsl_matrix_set(S, 0, 0, 2); gsl_matrix_set(S, 0, 1, 1);
    gsl_matrix_set(S, 1, 0, 1); gsl_matrix_set(S, 1, 1, 2);
    void *eval = gsl_vector_alloc(2);
    void *ews = gsl_eigen_symm_alloc(2);
    gsl_eigen_symm(S, eval, ews);   /* note: destroys S */
    printf("gsl_eigen_symm -> %g, %g   (expect 1 and 3, any order)\n",
           gsl_vector_get(eval, 0), gsl_vector_get(eval, 1));
    gsl_eigen_symm_free(ews); gsl_matrix_free(S); gsl_vector_free(eval);

    /* Tier-2 QR: solve the same A x = b (A=[[2,1],[1,3]], b=[3,5]) -> x=[0.8,1.4] via QR. */
    void *Aq = gsl_matrix_alloc(2, 2);
    gsl_matrix_set(Aq, 0, 0, 2); gsl_matrix_set(Aq, 0, 1, 1);
    gsl_matrix_set(Aq, 1, 0, 1); gsl_matrix_set(Aq, 1, 1, 3);
    void *tau = gsl_vector_alloc(2);
    void *bq = gsl_vector_alloc(2); gsl_vector_set(bq, 0, 3); gsl_vector_set(bq, 1, 5);
    void *xq = gsl_vector_alloc(2);
    gsl_linalg_QR_decomp(Aq, tau);
    gsl_linalg_QR_solve(Aq, tau, bq, xq);
    printf("gsl_linalg_QR_solve -> x0=%g x1=%g   (expect 0.8, 1.4)\n",
           gsl_vector_get(xq, 0), gsl_vector_get(xq, 1));
    gsl_matrix_free(Aq); gsl_vector_free(tau); gsl_vector_free(bq); gsl_vector_free(xq);

    /* opaque handle: alloc -> set -> sort -> get -> free, entirely sandboxed. The vector
     * lives in guest memory; `v` is only ever an opaque token. */
    double vin[5] = {3.1, 1.2, 4.8, 1.5, 2.6};
    void *v = gsl_vector_alloc(5);
    for (size_t i = 0; i < 5; i++) gsl_vector_set(v, i, vin[i]);
    /* Negative control: set LIND_BREAK_HANDLE=1 to corrupt the token before sorting. The
     * handle table must FAIL CLOSED ("invalid handle"), not silently pass — proving the
     * token is validated and a forged/stale handle never reaches a sandboxed call. */
    if (getenv("LIND_BREAK_HANDLE")) {
        fprintf(stderr, "[demo] corrupting handle (expect a fail-closed abort below)\n");
        v = (void *)((size_t)v ^ (size_t)0xdeadbeef);
    }
    gsl_sort_vector(v);
    printf("gsl_sort_vector -> %g %g %g %g %g   (expect 1.2 1.5 2.6 3.1 4.8)\n",
           gsl_vector_get(v, 0), gsl_vector_get(v, 1), gsl_vector_get(v, 2),
           gsl_vector_get(v, 3), gsl_vector_get(v, 4));
    gsl_vector_free(v);
    return 0;
}

