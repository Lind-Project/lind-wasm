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
/* Tier-2b rng: the generator is an opaque handle; gsl_rng_mt19937 is a library-owned TYPE
 * descriptor the app passes opaquely (never dereferenced) — declared void* so no GSL header
 * is pulled in. gsl_rng returns `unsigned long` (32-bit in the guest, masked on the host). */
extern void   *gsl_rng_mt19937;
void          *gsl_rng_alloc(void *T);
void           gsl_rng_set(void *r, unsigned long seed);
void           gsl_rng_free(void *r);
unsigned long  gsl_rng_get(void *r);
double         gsl_rng_uniform(void *r);
unsigned long  gsl_rng_uniform_int(void *r, unsigned long n);
unsigned long  gsl_rng_max(void *r);
unsigned long  gsl_rng_min(void *r);
double         gsl_ran_gaussian(void *r, double sigma);
/* Stage A: interpolation/spline. gsl_interp_cspline is a library-owned TYPE descriptor passed
 * opaquely (same trick as rng); the spline + accelerator are opaque handles. The accelerator
 * may be NULL (passes through as a guest NULL). */
extern void *gsl_interp_cspline;
void   *gsl_interp_accel_alloc(void);
void    gsl_interp_accel_free(void *a);
void   *gsl_spline_alloc(void *T, size_t n);
int     gsl_spline_init(void *spline, const double xa[], const double ya[], size_t n);
double  gsl_spline_eval(void *spline, double x, void *acc);
int     gsl_spline_eval_e(void *spline, double x, void *acc, double *y);
double  gsl_spline_eval_deriv(void *spline, double x, void *acc);
double  gsl_spline_eval_integ(void *spline, double a, double b, void *acc);
void    gsl_spline_free(void *spline);
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

static double dabs(double v) { return v < 0 ? -v : v; }


/* Stage F: wavelet transforms (no callbacks). Type pointer + workspace handle + in-place array. */
extern void *gsl_wavelet_daubechies;
void *gsl_wavelet_alloc(void *T, size_t k);
void  gsl_wavelet_free(void *w);
void *gsl_wavelet_workspace_alloc(size_t n);
void  gsl_wavelet_workspace_free(void *w);
int gsl_wavelet_transform_forward(void *w, double *data, size_t stride, size_t n, void *work);
int gsl_wavelet_transform_inverse(void *w, double *data, size_t stride, size_t n, void *work);
/* Stage H: 2-D wavelet (reuses the wavelet/workspace handles + the gsl_matrix handle). */
extern void *gsl_wavelet_haar;
int gsl_wavelet2d_transform_forward(void *w, double *data, size_t tda, size_t size1, size_t size2, void *work);
int gsl_wavelet2d_transform_inverse(void *w, double *data, size_t tda, size_t size1, size_t size2, void *work);
int gsl_wavelet2d_transform_matrix_forward(void *w, void *a, void *work);
int gsl_wavelet2d_transform_matrix_inverse(void *w, void *a, void *work);
/* Stage I: quasi-random sequences. */
extern void *gsl_qrng_sobol;
void *gsl_qrng_alloc(void *T, unsigned int d);
void  gsl_qrng_free(void *q);
int   gsl_qrng_get(void *q, double x[]);
/* Stage G: running statistics (single workspace handle). */
void  *gsl_rstat_alloc(void);
void   gsl_rstat_free(void *w);
size_t gsl_rstat_n(void *w);
int    gsl_rstat_add(double x, void *w);
double gsl_rstat_mean(void *w);
double gsl_rstat_variance(void *w);
double gsl_rstat_sd(void *w);
double gsl_rstat_min(void *w);
double gsl_rstat_max(void *w);

/* Stage C: FFT. Radix-2 transforms are in-place over the caller's array; mixed-radix adds a
 * wavetable + workspace handle for arbitrary n. gsl_complex_packed_array = 2n interleaved doubles. */
int gsl_fft_complex_radix2_forward(double *data, size_t stride, size_t n);
int gsl_fft_complex_radix2_inverse(double *data, size_t stride, size_t n);
int gsl_fft_real_radix2_transform(double *data, size_t stride, size_t n);
int gsl_fft_halfcomplex_radix2_inverse(double *data, size_t stride, size_t n);
void *gsl_fft_complex_wavetable_alloc(size_t n);
void  gsl_fft_complex_wavetable_free(void *w);
void *gsl_fft_complex_workspace_alloc(size_t n);
void  gsl_fft_complex_workspace_free(void *w);
int gsl_fft_complex_forward(double *data, size_t stride, size_t n, void *wavetable, void *work);
int gsl_fft_complex_inverse(double *data, size_t stride, size_t n, void *wavetable, void *work);
/* Stage C2: real/halfcomplex mixed-radix (arbitrary n) — own wavetables + shared workspace. */
void *gsl_fft_real_wavetable_alloc(size_t n);
void  gsl_fft_real_wavetable_free(void *w);
void *gsl_fft_halfcomplex_wavetable_alloc(size_t n);
void  gsl_fft_halfcomplex_wavetable_free(void *w);
void *gsl_fft_real_workspace_alloc(size_t n);
void  gsl_fft_real_workspace_free(void *w);
int gsl_fft_real_transform(double *data, size_t stride, size_t n, void *wt, void *work);
int gsl_fft_halfcomplex_inverse(double *data, size_t stride, size_t n, void *wt, void *work);
int gsl_fft_real_unpack(const double *rc, double *cc, size_t stride, size_t n);

/* Stage D: BLAS over the sandboxed gsl_vector/gsl_matrix handles. CBLAS enums are plain ints. */
#define CblasNoTrans 111
int    gsl_blas_ddot(void *X, void *Y, double *result);
double gsl_blas_dnrm2(void *X);
int    gsl_blas_dgemv(int TransA, double alpha, void *A, void *X, double beta, void *Y);
int    gsl_blas_dgemm(int TransA, int TransB, double alpha, void *A, void *B, double beta, void *C);
/* Stage D2: single-precision BLAS over gsl_vector_float / gsl_matrix_float handles. */
void  *gsl_vector_float_alloc(size_t n);
void   gsl_vector_float_set(void *v, size_t i, float x);
float  gsl_vector_float_get(const void *v, size_t i);
void   gsl_vector_float_free(void *v);
void  *gsl_matrix_float_alloc(size_t n1, size_t n2);
void   gsl_matrix_float_set(void *m, size_t i, size_t j, float x);
void   gsl_matrix_float_free(void *m);
int    gsl_blas_sdot(void *X, void *Y, float *result);
float  gsl_blas_snrm2(void *X);
int    gsl_blas_sgemv(int TransA, float alpha, void *A, void *X, float beta, void *Y);

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

    /* Tier-2b rng: the TYPE pointer (gsl_rng_mt19937) is a library-owned descriptor passed
     * as an opaque index; the generator object itself is an opaque handle living in the guest.
     * Validate by determinism: two mt19937 seeded alike must produce identical streams, and
     * gsl_rng_uniform must land in [0,1). No GSL internals are hardcoded. */
    void *r1 = gsl_rng_alloc(gsl_rng_mt19937);
    void *r2 = gsl_rng_alloc(gsl_rng_mt19937);
    gsl_rng_set(r1, 42);
    gsl_rng_set(r2, 42);
    unsigned long g1 = gsl_rng_get(r1), g2 = gsl_rng_get(r2);
    double u1 = gsl_rng_uniform(r1), u2 = gsl_rng_uniform(r2);
    unsigned long i1 = gsl_rng_uniform_int(r1, 100), i2 = gsl_rng_uniform_int(r2, 100);
    printf("gsl_rng mt19937: get=%lu uniform=%.6f int[0,100)=%lu   (expect both streams equal, uniform in [0,1))\n",
           g1, u1, i1);
    printf("gsl_rng determinism: %s   uniform range: %s\n",
           (g1 == g2 && u1 == u2 && i1 == i2) ? "OK (identical)" : "FAIL (diverged)",
           (u1 >= 0.0 && u1 < 1.0) ? "OK" : "FAIL");
    printf("gsl_rng max=%lu min=%lu\n", gsl_rng_max(r1), gsl_rng_min(r1));
    /* sampler over the rng handle: a Gaussian draw (just prove it runs & is finite). */
    double gz = gsl_ran_gaussian(r1, 2.0);
    printf("gsl_ran_gaussian(sigma=2) -> %g   (finite draw)\n", gz);
    gsl_rng_free(r1); gsl_rng_free(r2);

    /* Stage A spline: a natural cubic spline through COLLINEAR data (y = 3x + 2) reproduces the
     * straight line exactly, so eval/deriv/integ have known analytic values — a self-check that
     * does not depend on GSL internals. Data lives in the guest's spline object after init. */
    double sx[5] = {0.0, 1.0, 2.0, 3.0, 4.0};
    double sy[5] = {2.0, 5.0, 8.0, 11.0, 14.0};   /* 3x + 2 */
    void *acc = gsl_interp_accel_alloc();
    void *spl = gsl_spline_alloc(gsl_interp_cspline, 5);
    gsl_spline_init(spl, sx, sy, 5);
    double sv = gsl_spline_eval(spl, 1.5, acc);           /* expect 3*1.5+2 = 6.5   */
    double sd = gsl_spline_eval_deriv(spl, 1.5, acc);     /* expect 3              */
    double si = gsl_spline_eval_integ(spl, 0.0, 4.0, acc);/* expect 1.5*16 + 2*4 = 32 */
    double se; int sst = gsl_spline_eval_e(spl, 2.5, NULL, &se);  /* NULL accel; expect 9.5 */
    int ok = (sv > 6.5 - 1e-6 && sv < 6.5 + 1e-6) &&
             (sd > 3.0 - 1e-6 && sd < 3.0 + 1e-6) &&
             (si > 32.0 - 1e-6 && si < 32.0 + 1e-6) &&
             sst == 0 && (se > 9.5 - 1e-6 && se < 9.5 + 1e-6);
    printf("gsl_spline cspline: eval(1.5)=%g deriv=%g integ[0,4]=%g eval_e(2.5)=%g -> %s\n",
           sv, sd, si, se, ok ? "OK" : "FAIL");
    gsl_spline_free(spl); gsl_interp_accel_free(acc);

    
    
    
    /* Stage C FFT (a) complex radix-2, n=8: a constant signal transforms to a single DC spike
     * (bin 0 = n, rest 0); and forward+inverse round-trips to the original (GSL inverse is
     * normalized). The transform runs IN PLACE on the caller's array via Arg::InOut. */
    double cd[16];
    for (size_t k = 0; k < 8; k++) { cd[2 * k] = 1.0; cd[2 * k + 1] = 0.0; }
    gsl_fft_complex_radix2_forward(cd, 1, 8);
    int dcok = dabs(cd[0] - 8.0) < 1e-9 && dabs(cd[1]) < 1e-9 && dabs(cd[2]) < 1e-9 && dabs(cd[4]) < 1e-9;
    double corig[16], crt[16];
    for (size_t k = 0; k < 16; k++) { corig[k] = (double)((k * 7) % 13) - 6.0; crt[k] = corig[k]; }
    gsl_fft_complex_radix2_forward(crt, 1, 8);
    gsl_fft_complex_radix2_inverse(crt, 1, 8);
    int crok = 1;
    for (size_t k = 0; k < 16; k++) if (dabs(crt[k] - corig[k]) > 1e-9) crok = 0;
    printf("gsl_fft complex radix2: DC=%g (expect 8) roundtrip=%s -> %s\n",
           cd[0], crok ? "ok" : "fail", (dcok && crok) ? "OK" : "FAIL");

    /* (b) real radix-2, n=8: forward then halfcomplex inverse round-trips to the original. */
    double rd[8], rorig[8];
    for (size_t k = 0; k < 8; k++) { rorig[k] = (double)((k * 3) % 5); rd[k] = rorig[k]; }
    gsl_fft_real_radix2_transform(rd, 1, 8);
    gsl_fft_halfcomplex_radix2_inverse(rd, 1, 8);
    int reok = 1;
    for (size_t k = 0; k < 8; k++) if (dabs(rd[k] - rorig[k]) > 1e-9) reok = 0;
    printf("gsl_fft real radix2 roundtrip (n=8) -> %s\n", reok ? "OK" : "FAIL");

    /* (c) mixed-radix complex, n=6 (not a power of two): needs a wavetable + workspace handle.
     * forward then inverse round-trips to the original. */
    void *wt = gsl_fft_complex_wavetable_alloc(6);
    void *ws = gsl_fft_complex_workspace_alloc(6);
    double md[12], morig[12];
    for (size_t k = 0; k < 12; k++) { morig[k] = (double)((k * 5) % 7) - 3.0; md[k] = morig[k]; }
    gsl_fft_complex_forward(md, 1, 6, wt, ws);
    gsl_fft_complex_inverse(md, 1, 6, wt, ws);
    int mrok = 1;
    for (size_t k = 0; k < 12; k++) if (dabs(md[k] - morig[k]) > 1e-9) mrok = 0;
    gsl_fft_complex_workspace_free(ws);
    gsl_fft_complex_wavetable_free(wt);
    printf("gsl_fft complex mixed-radix (n=6) roundtrip -> %s\n", mrok ? "OK" : "FAIL");

    /* Stage C2 real mixed-radix, n=6 (not a power of two): own wavetables + a shared real
     * workspace. (a) all-ones -> DC coefficient = n; (b) unpack the packed coefficients to a full
     * complex array -> complex DC = n, imag 0; (c) forward (real) + inverse (halfcomplex)
     * round-trips to the original. */
    void *rwt = gsl_fft_real_wavetable_alloc(6);
    void *hcwt = gsl_fft_halfcomplex_wavetable_alloc(6);
    void *rws = gsl_fft_real_workspace_alloc(6);
    double ad[6];
    for (size_t k = 0; k < 6; k++) ad[k] = 1.0;
    gsl_fft_real_transform(ad, 1, 6, rwt, rws);          /* DC (ad[0]) = 6 */
    double cc[12];
    gsl_fft_real_unpack(ad, cc, 1, 6);                   /* complex DC: cc[0]=6, cc[1]=0 */
    double rsig[6], rsig0[6];
    for (size_t k = 0; k < 6; k++) { rsig0[k] = (double)((k * 5) % 7) - 3.0; rsig[k] = rsig0[k]; }
    gsl_fft_real_transform(rsig, 1, 6, rwt, rws);
    gsl_fft_halfcomplex_inverse(rsig, 1, 6, hcwt, rws);
    int rmr = 1;
    for (size_t k = 0; k < 6; k++) if (dabs(rsig[k] - rsig0[k]) > 1e-9) rmr = 0;
    gsl_fft_real_workspace_free(rws); gsl_fft_halfcomplex_wavetable_free(hcwt); gsl_fft_real_wavetable_free(rwt);
    printf("gsl_fft real mixed-radix (n=6): DC=%g (6) unpack_DC=%g (6) roundtrip=%s -> %s\n",
           ad[0], cc[0],
           rmr ? "ok" : "fail",
           (dabs(ad[0] - 6.0) < 1e-9 && dabs(cc[0] - 6.0) < 1e-9 && dabs(cc[1]) < 1e-9 && rmr) ? "OK" : "FAIL");

    /* Stage D BLAS: dense linear algebra over the SAME sandboxed vector/matrix handles. ddot,
     * dnrm2 (level 1), dgemv (level 2), dgemm (level 3) — each self-checked against a hand value. */
    void *bx = gsl_vector_alloc(3); gsl_vector_set(bx, 0, 1); gsl_vector_set(bx, 1, 2); gsl_vector_set(bx, 2, 3);
    void *by = gsl_vector_alloc(3); gsl_vector_set(by, 0, 4); gsl_vector_set(by, 1, 5); gsl_vector_set(by, 2, 6);
    double dot = 0; gsl_blas_ddot(bx, by, &dot);          /* 1*4+2*5+3*6 = 32 */
    double nrm = gsl_blas_dnrm2(bx);                      /* sqrt(14) = 3.7416574 */
    void *bA = gsl_matrix_alloc(2, 2);
    gsl_matrix_set(bA, 0, 0, 1); gsl_matrix_set(bA, 0, 1, 2); gsl_matrix_set(bA, 1, 0, 3); gsl_matrix_set(bA, 1, 1, 4);
    void *vx = gsl_vector_alloc(2); gsl_vector_set(vx, 0, 1); gsl_vector_set(vx, 1, 1);
    void *vy = gsl_vector_alloc(2);
    gsl_blas_dgemv(CblasNoTrans, 1.0, bA, vx, 0.0, vy);   /* A*[1,1] = [3,7] */
    void *bB = gsl_matrix_alloc(2, 2);
    gsl_matrix_set(bB, 0, 0, 1); gsl_matrix_set(bB, 0, 1, 0); gsl_matrix_set(bB, 1, 0, 0); gsl_matrix_set(bB, 1, 1, 1);
    void *bC = gsl_matrix_alloc(2, 2);
    gsl_blas_dgemm(CblasNoTrans, CblasNoTrans, 1.0, bA, bB, 0.0, bC);  /* A*I = A */
    int blok = dabs(dot - 32.0) < 1e-9 && dabs(nrm - 3.7416573867739413) < 1e-6 &&
               dabs(gsl_vector_get(vy, 0) - 3.0) < 1e-9 && dabs(gsl_vector_get(vy, 1) - 7.0) < 1e-9 &&
               dabs(gsl_matrix_get(bC, 0, 0) - 1.0) < 1e-9 && dabs(gsl_matrix_get(bC, 1, 1) - 4.0) < 1e-9;
    printf("gsl_blas: ddot=%g (32) dnrm2=%g (3.7417) dgemv=[%g,%g] ([3,7]) dgemm C[1,1]=%g (4) -> %s\n",
           dot, nrm, gsl_vector_get(vy, 0), gsl_vector_get(vy, 1), gsl_matrix_get(bC, 1, 1), blok ? "OK" : "FAIL");
    gsl_vector_free(bx); gsl_vector_free(by); gsl_vector_free(vx); gsl_vector_free(vy);
    gsl_matrix_free(bA); gsl_matrix_free(bB); gsl_matrix_free(bC);

    /* Stage D2 single-precision BLAS: the same operations over float handles (classes 8/5). */
    void *fx = gsl_vector_float_alloc(3); gsl_vector_float_set(fx, 0, 1); gsl_vector_float_set(fx, 1, 2); gsl_vector_float_set(fx, 2, 3);
    void *fy = gsl_vector_float_alloc(3); gsl_vector_float_set(fy, 0, 4); gsl_vector_float_set(fy, 1, 5); gsl_vector_float_set(fy, 2, 6);
    float fdot = 0; gsl_blas_sdot(fx, fy, &fdot);     /* 32 */
    float fnrm = gsl_blas_snrm2(fx);                  /* sqrt(14) = 3.74166 */
    void *fA = gsl_matrix_float_alloc(2, 2);
    gsl_matrix_float_set(fA, 0, 0, 1); gsl_matrix_float_set(fA, 0, 1, 2); gsl_matrix_float_set(fA, 1, 0, 3); gsl_matrix_float_set(fA, 1, 1, 4);
    void *fvx = gsl_vector_float_alloc(2); gsl_vector_float_set(fvx, 0, 1); gsl_vector_float_set(fvx, 1, 1);
    void *fvy = gsl_vector_float_alloc(2);
    gsl_blas_sgemv(CblasNoTrans, 1.0f, fA, fvx, 0.0f, fvy);   /* A*[1,1] = [3,7] */
    int sok = dabs((double)fdot - 32.0) < 1e-3 && dabs((double)fnrm - 3.7416574) < 1e-3 &&
              dabs((double)gsl_vector_float_get(fvy, 0) - 3.0) < 1e-3 &&
              dabs((double)gsl_vector_float_get(fvy, 1) - 7.0) < 1e-3;
    printf("gsl_blas (single): sdot=%g (32) snrm2=%g (3.7417) sgemv=[%g,%g] ([3,7]) -> %s\n",
           (double)fdot, (double)fnrm, (double)gsl_vector_float_get(fvy, 0), (double)gsl_vector_float_get(fvy, 1), sok ? "OK" : "FAIL");
    gsl_vector_float_free(fx); gsl_vector_float_free(fy); gsl_vector_float_free(fvx); gsl_vector_float_free(fvy);
    gsl_matrix_float_free(fA);

    
    /* Stage F wavelet: a Daubechies-4 forward then inverse transform round-trips to the original
     * (n=8, a power of two). Type descriptor is opaque; the wavelet and workspace are handles;
     * the transform runs in place via Arg::InOut. */
    void *wav = gsl_wavelet_alloc(gsl_wavelet_daubechies, 4);
    void *wws = gsl_wavelet_workspace_alloc(8);
    double wd[8], wo2[8];
    for (size_t k = 0; k < 8; k++) { wo2[k] = (double)((k * 3) % 5) + 1.0; wd[k] = wo2[k]; }
    gsl_wavelet_transform_forward(wav, wd, 1, 8, wws);
    gsl_wavelet_transform_inverse(wav, wd, 1, 8, wws);
    int wok = 1;
    for (size_t k = 0; k < 8; k++) if (dabs(wd[k] - wo2[k]) > 1e-9) wok = 0;
    gsl_wavelet_workspace_free(wws); gsl_wavelet_free(wav);
    printf("gsl_wavelet daubechies-4 (n=8) forward+inverse roundtrip -> %s\n", wok ? "OK" : "FAIL");

    /* Stage H 2-D wavelet: a 4x4 Haar transform, both forms. (a) flat array (in place via
     * InOut); (b) over a gsl_matrix handle (all handles). Forward+inverse round-trips. */
    void *w2 = gsl_wavelet_alloc(gsl_wavelet_haar, 2);
    void *ws2 = gsl_wavelet_workspace_alloc(4);
    double m2d[16], m2o[16];
    for (size_t k = 0; k < 16; k++) { m2o[k] = (double)((k * 2) % 7) + 1.0; m2d[k] = m2o[k]; }
    gsl_wavelet2d_transform_forward(w2, m2d, 4, 4, 4, ws2);
    gsl_wavelet2d_transform_inverse(w2, m2d, 4, 4, 4, ws2);
    int w2a = 1;
    for (size_t k = 0; k < 16; k++) if (dabs(m2d[k] - m2o[k]) > 1e-9) w2a = 0;
    void *wm = gsl_matrix_alloc(4, 4);
    for (size_t i = 0; i < 4; i++) for (size_t j = 0; j < 4; j++) gsl_matrix_set(wm, i, j, (double)(i * 4 + j) + 1.0);
    gsl_wavelet2d_transform_matrix_forward(w2, wm, ws2);
    gsl_wavelet2d_transform_matrix_inverse(w2, wm, ws2);
    int w2b = 1;
    for (size_t i = 0; i < 4; i++) for (size_t j = 0; j < 4; j++)
        if (dabs(gsl_matrix_get(wm, i, j) - ((double)(i * 4 + j) + 1.0)) > 1e-9) w2b = 0;
    gsl_matrix_free(wm); gsl_wavelet_workspace_free(ws2); gsl_wavelet_free(w2);
    printf("gsl_wavelet2d haar 4x4: flat roundtrip=%s matrix roundtrip=%s -> %s\n",
           w2a ? "ok" : "fail", w2b ? "ok" : "fail", (w2a && w2b) ? "OK" : "FAIL");

    /* Stage G running statistics: stream {1,2,3,4,5} into the workspace and read the running
     * moments. mean=3, sample variance=2.5, n=5, min=1, max=5 (known values, no GSL internals). */
    void *rstw = gsl_rstat_alloc();
    double rdata[5] = {1.0, 2.0, 3.0, 4.0, 5.0};
    for (size_t k = 0; k < 5; k++) gsl_rstat_add(rdata[k], rstw);
    double rmean = gsl_rstat_mean(rstw), rvar = gsl_rstat_variance(rstw), rsd = gsl_rstat_sd(rstw);
    size_t rn = gsl_rstat_n(rstw);
    int rok = rn == 5 && dabs(rmean - 3.0) < 1e-12 && dabs(rvar - 2.5) < 1e-12 &&
              dabs(gsl_rstat_min(rstw) - 1.0) < 1e-12 && dabs(gsl_rstat_max(rstw) - 5.0) < 1e-12;
    gsl_rstat_free(rstw);
    printf("gsl_rstat: n=%zu mean=%g (3) var=%g (2.5) sd=%g -> %s\n", rn, rmean, rvar, rsd, rok ? "OK" : "FAIL");

    /* Stage I quasi-random: two Sobol generators (d=2) must produce identical sequences; points
     * land in [0,1); and successive points differ. Checks the sequence, not hardcoded values. */
    void *q1 = gsl_qrng_alloc(gsl_qrng_sobol, 2);
    void *q2 = gsl_qrng_alloc(gsl_qrng_sobol, 2);
    double p1[2], p2[2], p1b[2];
    gsl_qrng_get(q1, p1); gsl_qrng_get(q2, p2); gsl_qrng_get(q1, p1b);
    int qok = p1[0] == p2[0] && p1[1] == p2[1] &&
              p1[0] >= 0.0 && p1[0] < 1.0 && p1[1] >= 0.0 && p1[1] < 1.0 &&
              (p1b[0] != p1[0] || p1b[1] != p1[1]);
    gsl_qrng_free(q1); gsl_qrng_free(q2);
    printf("gsl_qrng sobol d=2: first=(%g,%g) second=(%g,%g) -> %s\n",
           p1[0], p1[1], p1b[0], p1b[1], qok ? "OK" : "FAIL");

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

