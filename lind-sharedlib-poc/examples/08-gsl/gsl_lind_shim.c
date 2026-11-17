// GSL shim — the resident MAIN module behind the preloaded libgsl.cwasm (same model as
// 06-openblas). It is the wasm main module; real GSL is loaded as a PRELOAD behind it.
//
// Two jobs:
//   1. Export guest_malloc / guest_free so the host stub can place inputs into the shared
//      linear memory and read results back out (marshalling).
//   2. Export one thin `lind_gsl_*` forwarder per GSL function we expose. These call the
//      real gsl_* symbols, which are UNDEFINED here -> the default (dynamic) lind_compile
//      build turns them into dynamic imports, resolved at runtime from the preloaded GSL.
//
// We deliberately do NOT #include <gsl/...> — several GSL functions (e.g. gsl_poly_eval)
// are INLINE in the headers and would compile into this module instead of importing from
// the preload. Forward-declare only the non-inline functions we wrap.

#include <stddef.h>
#include <stdlib.h>

extern int gsl_poly_solve_quadratic(double a, double b, double c, double *x0, double *x1);
extern int gsl_poly_solve_cubic(double a, double b, double c, double *x0, double *x1, double *x2);
extern int gsl_poly_eval_derivs(const double c[], size_t lenc, double x, double res[], size_t lenres);

// gsl_sort_vector belongs to the sort module; the gsl_vector object itself is owned by the
// generated vector module (generated/vector_shim.inc.c). Forward-declare the opaque struct
// (incomplete, pointer-only) just for this one forwarder.
struct gsl_vector;
extern void gsl_sort_vector(struct gsl_vector *v);

__attribute__((export_name("guest_malloc")))
void *guest_malloc(size_t n) { return malloc(n); }

__attribute__((export_name("guest_free")))
void guest_free(void *p) { free(p); }

__attribute__((export_name("lind_gsl_poly_solve_quadratic")))
int lind_gsl_poly_solve_quadratic(double a, double b, double c, double *x0, double *x1) {
    return gsl_poly_solve_quadratic(a, b, c, x0, x1);
}

__attribute__((export_name("lind_gsl_poly_solve_cubic")))
int lind_gsl_poly_solve_cubic(double a, double b, double c, double *x0, double *x1, double *x2) {
    return gsl_poly_solve_cubic(a, b, c, x0, x1, x2);
}

__attribute__((export_name("lind_gsl_poly_eval_derivs")))
int lind_gsl_poly_eval_derivs(const double c[], size_t lenc, double x, double res[], size_t lenres) {
    return gsl_poly_eval_derivs(c, lenc, x, res, lenres);
}

// gsl_sort_vector forwarder (the gsl_vector module itself is generated; see
// generated/vector_shim.inc.c). Passes the guest pointer straight through.
__attribute__((export_name("lind_gsl_sort_vector")))
void lind_gsl_sort_vector(struct gsl_vector *v) { gsl_sort_vector(v); }

// GSL sort module — generated forwarders (see gen_gsl_sort.py).
#include "generated/sort_shim.inc.c"

// GSL statistics module — generated forwarders (see gen_gsl_stats.py).
#include "generated/stats_shim.inc.c"

// GSL scalar modules (cdf / randist pdf / sys) — generated forwarders (see gen_gsl_scalar.py).
#include "generated/scalar_shim.inc.c"

// GSL special-function `_e` forms (gsl_sf_*_e) — generated forwarders (see gen_gsl_sf_result.py).
#include "generated/sf_result_shim.inc.c"

// GSL opaque-handle modules (Tier 2) — generated forwarders (see gen_gsl_handle.py).
#include "generated/vector_shim.inc.c"
#include "generated/vector_float_shim.inc.c"
#include "generated/vector_int_shim.inc.c"
#include "generated/vector_uint_shim.inc.c"
#include "generated/matrix_shim.inc.c"
#include "generated/matrix_float_shim.inc.c"
#include "generated/matrix_int_shim.inc.c"
#include "generated/matrix_uint_shim.inc.c"
#include "generated/permutation_shim.inc.c"
#include "generated/combination_shim.inc.c"
#include "generated/histogram_shim.inc.c"
#include "generated/linalg_shim.inc.c"
#include "generated/eigen_symm_ws_shim.inc.c"
#include "generated/eigen_symmv_ws_shim.inc.c"
#include "generated/eigen_shim.inc.c"
#include "generated/multifit_linear_ws_shim.inc.c"
#include "generated/multifit_shim.inc.c"
#include "generated/rng_shim.inc.c"
#include "generated/ran_shim.inc.c"
#include "generated/interp_accel_shim.inc.c"
#include "generated/spline_shim.inc.c"
#include "generated/blas_shim.inc.c"
#include "generated/blas_float_shim.inc.c"

// ---------------------------------------------------------------------------
// Stage C: FFT (hand-written). Radix-2 transforms are IN-PLACE over the caller's array
// (Arg::InOut on the stub side): data is copied into the guest, transformed, copied back.
// Mixed-radix (arbitrary n) adds a wavetable + workspace opaque handle, same as other Tier-2
// workspaces. `gsl_complex_packed_array` is just `double *` (2n interleaved re,im). No engine
// change. (The direction enum is passed as a plain int.)
// ---------------------------------------------------------------------------
typedef struct gsl_fft_complex_wavetable_s gsl_fft_complex_wavetable;
typedef struct gsl_fft_complex_workspace_s gsl_fft_complex_workspace;

extern int gsl_fft_complex_radix2_forward(double *data, size_t stride, size_t n);
extern int gsl_fft_complex_radix2_backward(double *data, size_t stride, size_t n);
extern int gsl_fft_complex_radix2_inverse(double *data, size_t stride, size_t n);
extern int gsl_fft_complex_radix2_transform(double *data, size_t stride, size_t n, int sign);
extern int gsl_fft_real_radix2_transform(double *data, size_t stride, size_t n);
extern int gsl_fft_halfcomplex_radix2_inverse(double *data, size_t stride, size_t n);
extern int gsl_fft_halfcomplex_radix2_backward(double *data, size_t stride, size_t n);

__attribute__((export_name("lind_gsl_fft_complex_radix2_forward")))
int lind_gsl_fft_complex_radix2_forward(double *data, size_t stride, size_t n) { return gsl_fft_complex_radix2_forward(data, stride, n); }
__attribute__((export_name("lind_gsl_fft_complex_radix2_backward")))
int lind_gsl_fft_complex_radix2_backward(double *data, size_t stride, size_t n) { return gsl_fft_complex_radix2_backward(data, stride, n); }
__attribute__((export_name("lind_gsl_fft_complex_radix2_inverse")))
int lind_gsl_fft_complex_radix2_inverse(double *data, size_t stride, size_t n) { return gsl_fft_complex_radix2_inverse(data, stride, n); }
__attribute__((export_name("lind_gsl_fft_complex_radix2_transform")))
int lind_gsl_fft_complex_radix2_transform(double *data, size_t stride, size_t n, int sign) { return gsl_fft_complex_radix2_transform(data, stride, n, sign); }
__attribute__((export_name("lind_gsl_fft_real_radix2_transform")))
int lind_gsl_fft_real_radix2_transform(double *data, size_t stride, size_t n) { return gsl_fft_real_radix2_transform(data, stride, n); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_radix2_inverse")))
int lind_gsl_fft_halfcomplex_radix2_inverse(double *data, size_t stride, size_t n) { return gsl_fft_halfcomplex_radix2_inverse(data, stride, n); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_radix2_backward")))
int lind_gsl_fft_halfcomplex_radix2_backward(double *data, size_t stride, size_t n) { return gsl_fft_halfcomplex_radix2_backward(data, stride, n); }

extern gsl_fft_complex_wavetable *gsl_fft_complex_wavetable_alloc(size_t n);
extern void gsl_fft_complex_wavetable_free(gsl_fft_complex_wavetable *w);
extern gsl_fft_complex_workspace *gsl_fft_complex_workspace_alloc(size_t n);
extern void gsl_fft_complex_workspace_free(gsl_fft_complex_workspace *w);
extern int gsl_fft_complex_forward(double *data, size_t stride, size_t n, const gsl_fft_complex_wavetable *w, gsl_fft_complex_workspace *work);
extern int gsl_fft_complex_backward(double *data, size_t stride, size_t n, const gsl_fft_complex_wavetable *w, gsl_fft_complex_workspace *work);
extern int gsl_fft_complex_inverse(double *data, size_t stride, size_t n, const gsl_fft_complex_wavetable *w, gsl_fft_complex_workspace *work);

__attribute__((export_name("lind_gsl_fft_complex_wavetable_alloc")))
gsl_fft_complex_wavetable *lind_gsl_fft_complex_wavetable_alloc(size_t n) { return gsl_fft_complex_wavetable_alloc(n); }
__attribute__((export_name("lind_gsl_fft_complex_wavetable_free")))
void lind_gsl_fft_complex_wavetable_free(gsl_fft_complex_wavetable *w) { gsl_fft_complex_wavetable_free(w); }
__attribute__((export_name("lind_gsl_fft_complex_workspace_alloc")))
gsl_fft_complex_workspace *lind_gsl_fft_complex_workspace_alloc(size_t n) { return gsl_fft_complex_workspace_alloc(n); }
__attribute__((export_name("lind_gsl_fft_complex_workspace_free")))
void lind_gsl_fft_complex_workspace_free(gsl_fft_complex_workspace *w) { gsl_fft_complex_workspace_free(w); }
__attribute__((export_name("lind_gsl_fft_complex_forward")))
int lind_gsl_fft_complex_forward(double *data, size_t stride, size_t n, gsl_fft_complex_wavetable *w, gsl_fft_complex_workspace *work) { return gsl_fft_complex_forward(data, stride, n, w, work); }
__attribute__((export_name("lind_gsl_fft_complex_backward")))
int lind_gsl_fft_complex_backward(double *data, size_t stride, size_t n, gsl_fft_complex_wavetable *w, gsl_fft_complex_workspace *work) { return gsl_fft_complex_backward(data, stride, n, w, work); }
__attribute__((export_name("lind_gsl_fft_complex_inverse")))
int lind_gsl_fft_complex_inverse(double *data, size_t stride, size_t n, gsl_fft_complex_wavetable *w, gsl_fft_complex_workspace *work) { return gsl_fft_complex_inverse(data, stride, n, w, work); }

// ---------------------------------------------------------------------------
// Stage C2: real / halfcomplex MIXED-RADIX FFT (arbitrary n). Own wavetables (real +
// halfcomplex) and a SHARED real workspace; transforms are in-place (InOut, n reals). unpack
// expands a packed coefficient array (n reals) into a full complex array (2n doubles).
// ---------------------------------------------------------------------------
typedef struct gsl_fft_real_wavetable_s        gsl_fft_real_wavetable;
typedef struct gsl_fft_halfcomplex_wavetable_s gsl_fft_halfcomplex_wavetable;
typedef struct gsl_fft_real_workspace_s        gsl_fft_real_workspace;

extern gsl_fft_real_wavetable *gsl_fft_real_wavetable_alloc(size_t n);
extern void gsl_fft_real_wavetable_free(gsl_fft_real_wavetable *w);
extern gsl_fft_halfcomplex_wavetable *gsl_fft_halfcomplex_wavetable_alloc(size_t n);
extern void gsl_fft_halfcomplex_wavetable_free(gsl_fft_halfcomplex_wavetable *w);
extern gsl_fft_real_workspace *gsl_fft_real_workspace_alloc(size_t n);
extern void gsl_fft_real_workspace_free(gsl_fft_real_workspace *w);
extern int gsl_fft_real_transform(double *data, size_t stride, size_t n, const gsl_fft_real_wavetable *wt, gsl_fft_real_workspace *work);
extern int gsl_fft_halfcomplex_transform(double *data, size_t stride, size_t n, const gsl_fft_halfcomplex_wavetable *wt, gsl_fft_real_workspace *work);
extern int gsl_fft_halfcomplex_backward(double *data, size_t stride, size_t n, const gsl_fft_halfcomplex_wavetable *wt, gsl_fft_real_workspace *work);
extern int gsl_fft_halfcomplex_inverse(double *data, size_t stride, size_t n, const gsl_fft_halfcomplex_wavetable *wt, gsl_fft_real_workspace *work);
extern int gsl_fft_real_unpack(const double *rc, double *cc, size_t stride, size_t n);
extern int gsl_fft_halfcomplex_unpack(const double *hc, double *cc, size_t stride, size_t n);

__attribute__((export_name("lind_gsl_fft_real_wavetable_alloc")))
gsl_fft_real_wavetable *lind_gsl_fft_real_wavetable_alloc(size_t n) { return gsl_fft_real_wavetable_alloc(n); }
__attribute__((export_name("lind_gsl_fft_real_wavetable_free")))
void lind_gsl_fft_real_wavetable_free(gsl_fft_real_wavetable *w) { gsl_fft_real_wavetable_free(w); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_wavetable_alloc")))
gsl_fft_halfcomplex_wavetable *lind_gsl_fft_halfcomplex_wavetable_alloc(size_t n) { return gsl_fft_halfcomplex_wavetable_alloc(n); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_wavetable_free")))
void lind_gsl_fft_halfcomplex_wavetable_free(gsl_fft_halfcomplex_wavetable *w) { gsl_fft_halfcomplex_wavetable_free(w); }
__attribute__((export_name("lind_gsl_fft_real_workspace_alloc")))
gsl_fft_real_workspace *lind_gsl_fft_real_workspace_alloc(size_t n) { return gsl_fft_real_workspace_alloc(n); }
__attribute__((export_name("lind_gsl_fft_real_workspace_free")))
void lind_gsl_fft_real_workspace_free(gsl_fft_real_workspace *w) { gsl_fft_real_workspace_free(w); }
__attribute__((export_name("lind_gsl_fft_real_transform")))
int lind_gsl_fft_real_transform(double *data, size_t stride, size_t n, gsl_fft_real_wavetable *wt, gsl_fft_real_workspace *work) { return gsl_fft_real_transform(data, stride, n, wt, work); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_transform")))
int lind_gsl_fft_halfcomplex_transform(double *data, size_t stride, size_t n, gsl_fft_halfcomplex_wavetable *wt, gsl_fft_real_workspace *work) { return gsl_fft_halfcomplex_transform(data, stride, n, wt, work); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_backward")))
int lind_gsl_fft_halfcomplex_backward(double *data, size_t stride, size_t n, gsl_fft_halfcomplex_wavetable *wt, gsl_fft_real_workspace *work) { return gsl_fft_halfcomplex_backward(data, stride, n, wt, work); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_inverse")))
int lind_gsl_fft_halfcomplex_inverse(double *data, size_t stride, size_t n, gsl_fft_halfcomplex_wavetable *wt, gsl_fft_real_workspace *work) { return gsl_fft_halfcomplex_inverse(data, stride, n, wt, work); }
__attribute__((export_name("lind_gsl_fft_real_unpack")))
int lind_gsl_fft_real_unpack(const double *rc, double *cc, size_t stride, size_t n) { return gsl_fft_real_unpack(rc, cc, stride, n); }
__attribute__((export_name("lind_gsl_fft_halfcomplex_unpack")))
int lind_gsl_fft_halfcomplex_unpack(const double *hc, double *cc, size_t stride, size_t n) { return gsl_fft_halfcomplex_unpack(hc, cc, stride, n); }

// ---------------------------------------------------------------------------
// Stage F: wavelet transforms (no callbacks). A type-pointer constructor
// gsl_wavelet_alloc(type, k), a workspace handle, and an in-place array transform (InOut).
// Combines the three proven non-callback mechanisms. No engine change.
// ---------------------------------------------------------------------------
typedef struct gsl_wavelet_type_s      gsl_wavelet_type;
typedef struct gsl_wavelet_s           gsl_wavelet;
typedef struct gsl_wavelet_workspace_s gsl_wavelet_workspace;

extern const gsl_wavelet_type *gsl_wavelet_daubechies;
extern const gsl_wavelet_type *gsl_wavelet_daubechies_centered;
extern const gsl_wavelet_type *gsl_wavelet_haar;
extern const gsl_wavelet_type *gsl_wavelet_haar_centered;
extern const gsl_wavelet_type *gsl_wavelet_bspline;
extern const gsl_wavelet_type *gsl_wavelet_bspline_centered;
extern gsl_wavelet *gsl_wavelet_alloc(const gsl_wavelet_type *T, size_t k);
extern void gsl_wavelet_free(gsl_wavelet *w);
extern gsl_wavelet_workspace *gsl_wavelet_workspace_alloc(size_t n);
extern void gsl_wavelet_workspace_free(gsl_wavelet_workspace *work);
extern int gsl_wavelet_transform_forward(const gsl_wavelet *w, double *data, size_t stride, size_t n, gsl_wavelet_workspace *work);
extern int gsl_wavelet_transform_inverse(const gsl_wavelet *w, double *data, size_t stride, size_t n, gsl_wavelet_workspace *work);

__attribute__((export_name("lind_gsl_wavelet_alloc")))
gsl_wavelet *lind_gsl_wavelet_alloc(int idx, size_t k) {
    const gsl_wavelet_type *tbl[] = { 0, gsl_wavelet_daubechies, gsl_wavelet_daubechies_centered,
                                      gsl_wavelet_haar, gsl_wavelet_haar_centered,
                                      gsl_wavelet_bspline, gsl_wavelet_bspline_centered };
    if (idx < 1 || idx > 6) return 0;
    return gsl_wavelet_alloc(tbl[idx], k);
}
__attribute__((export_name("lind_gsl_wavelet_free")))
void lind_gsl_wavelet_free(gsl_wavelet *w) { gsl_wavelet_free(w); }
__attribute__((export_name("lind_gsl_wavelet_workspace_alloc")))
gsl_wavelet_workspace *lind_gsl_wavelet_workspace_alloc(size_t n) { return gsl_wavelet_workspace_alloc(n); }
__attribute__((export_name("lind_gsl_wavelet_workspace_free")))
void lind_gsl_wavelet_workspace_free(gsl_wavelet_workspace *work) { gsl_wavelet_workspace_free(work); }
__attribute__((export_name("lind_gsl_wavelet_transform_forward")))
int lind_gsl_wavelet_transform_forward(gsl_wavelet *w, double *data, size_t stride, size_t n, gsl_wavelet_workspace *work) { return gsl_wavelet_transform_forward(w, data, stride, n, work); }
__attribute__((export_name("lind_gsl_wavelet_transform_inverse")))
int lind_gsl_wavelet_transform_inverse(gsl_wavelet *w, double *data, size_t stride, size_t n, gsl_wavelet_workspace *work) { return gsl_wavelet_transform_inverse(w, data, stride, n, work); }

// ---------------------------------------------------------------------------
// Stage G: running (streaming) statistics (gsl_rstat). A single workspace handle; values are
// added one at a time and the running moments read back. No callbacks, no engine change.
// (Note gsl_rstat_add takes the value FIRST, then the workspace.)
// ---------------------------------------------------------------------------
typedef struct gsl_rstat_workspace_s gsl_rstat_workspace;

extern gsl_rstat_workspace *gsl_rstat_alloc(void);
extern void   gsl_rstat_free(gsl_rstat_workspace *w);
extern size_t gsl_rstat_n(const gsl_rstat_workspace *w);
extern int    gsl_rstat_add(double x, gsl_rstat_workspace *w);
extern int    gsl_rstat_reset(gsl_rstat_workspace *w);
extern double gsl_rstat_min(const gsl_rstat_workspace *w);
extern double gsl_rstat_max(const gsl_rstat_workspace *w);
extern double gsl_rstat_mean(const gsl_rstat_workspace *w);
extern double gsl_rstat_variance(const gsl_rstat_workspace *w);
extern double gsl_rstat_sd(const gsl_rstat_workspace *w);
extern double gsl_rstat_rms(const gsl_rstat_workspace *w);
extern double gsl_rstat_skew(const gsl_rstat_workspace *w);
extern double gsl_rstat_kurtosis(const gsl_rstat_workspace *w);
extern double gsl_rstat_median(gsl_rstat_workspace *w);

__attribute__((export_name("lind_gsl_rstat_alloc")))
gsl_rstat_workspace *lind_gsl_rstat_alloc(void) { return gsl_rstat_alloc(); }
__attribute__((export_name("lind_gsl_rstat_free")))
void lind_gsl_rstat_free(gsl_rstat_workspace *w) { gsl_rstat_free(w); }
__attribute__((export_name("lind_gsl_rstat_n")))
size_t lind_gsl_rstat_n(gsl_rstat_workspace *w) { return gsl_rstat_n(w); }
__attribute__((export_name("lind_gsl_rstat_add")))
int lind_gsl_rstat_add(double x, gsl_rstat_workspace *w) { return gsl_rstat_add(x, w); }
__attribute__((export_name("lind_gsl_rstat_reset")))
int lind_gsl_rstat_reset(gsl_rstat_workspace *w) { return gsl_rstat_reset(w); }
__attribute__((export_name("lind_gsl_rstat_min")))
double lind_gsl_rstat_min(gsl_rstat_workspace *w) { return gsl_rstat_min(w); }
__attribute__((export_name("lind_gsl_rstat_max")))
double lind_gsl_rstat_max(gsl_rstat_workspace *w) { return gsl_rstat_max(w); }
__attribute__((export_name("lind_gsl_rstat_mean")))
double lind_gsl_rstat_mean(gsl_rstat_workspace *w) { return gsl_rstat_mean(w); }
__attribute__((export_name("lind_gsl_rstat_variance")))
double lind_gsl_rstat_variance(gsl_rstat_workspace *w) { return gsl_rstat_variance(w); }
__attribute__((export_name("lind_gsl_rstat_sd")))
double lind_gsl_rstat_sd(gsl_rstat_workspace *w) { return gsl_rstat_sd(w); }
__attribute__((export_name("lind_gsl_rstat_rms")))
double lind_gsl_rstat_rms(gsl_rstat_workspace *w) { return gsl_rstat_rms(w); }
__attribute__((export_name("lind_gsl_rstat_skew")))
double lind_gsl_rstat_skew(gsl_rstat_workspace *w) { return gsl_rstat_skew(w); }
__attribute__((export_name("lind_gsl_rstat_kurtosis")))
double lind_gsl_rstat_kurtosis(gsl_rstat_workspace *w) { return gsl_rstat_kurtosis(w); }
__attribute__((export_name("lind_gsl_rstat_median")))
double lind_gsl_rstat_median(gsl_rstat_workspace *w) { return gsl_rstat_median(w); }

// ---------------------------------------------------------------------------
// Stage H: 2-D wavelet transforms. Reuses the Stage F wavelet + workspace handles and the
// Tier-2 gsl_matrix handle. The flat-array variants transform a size1 x size2 matrix stored
// with physical row stride `tda` in place (InOut, size1*tda doubles); the matrix variants take
// a gsl_matrix handle directly (all handles, no array marshalling). No engine change.
// (gsl_wavelet / gsl_wavelet_workspace typedefs come from the Stage F block above;
//  struct gsl_matrix is forward-declared by the generated matrix shim.)
// ---------------------------------------------------------------------------
extern int gsl_wavelet2d_transform_forward(const gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_transform_inverse(const gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_nstransform_forward(const gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_nstransform_inverse(const gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_transform_matrix_forward(const gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_transform_matrix_inverse(const gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_nstransform_matrix_forward(const gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work);
extern int gsl_wavelet2d_nstransform_matrix_inverse(const gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work);

__attribute__((export_name("lind_gsl_wavelet2d_transform_forward")))
int lind_gsl_wavelet2d_transform_forward(gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work) { return gsl_wavelet2d_transform_forward(w, data, tda, size1, size2, work); }
__attribute__((export_name("lind_gsl_wavelet2d_transform_inverse")))
int lind_gsl_wavelet2d_transform_inverse(gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work) { return gsl_wavelet2d_transform_inverse(w, data, tda, size1, size2, work); }
__attribute__((export_name("lind_gsl_wavelet2d_nstransform_forward")))
int lind_gsl_wavelet2d_nstransform_forward(gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work) { return gsl_wavelet2d_nstransform_forward(w, data, tda, size1, size2, work); }
__attribute__((export_name("lind_gsl_wavelet2d_nstransform_inverse")))
int lind_gsl_wavelet2d_nstransform_inverse(gsl_wavelet *w, double *data, size_t tda, size_t size1, size_t size2, gsl_wavelet_workspace *work) { return gsl_wavelet2d_nstransform_inverse(w, data, tda, size1, size2, work); }
__attribute__((export_name("lind_gsl_wavelet2d_transform_matrix_forward")))
int lind_gsl_wavelet2d_transform_matrix_forward(gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work) { return gsl_wavelet2d_transform_matrix_forward(w, a, work); }
__attribute__((export_name("lind_gsl_wavelet2d_transform_matrix_inverse")))
int lind_gsl_wavelet2d_transform_matrix_inverse(gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work) { return gsl_wavelet2d_transform_matrix_inverse(w, a, work); }
__attribute__((export_name("lind_gsl_wavelet2d_nstransform_matrix_forward")))
int lind_gsl_wavelet2d_nstransform_matrix_forward(gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work) { return gsl_wavelet2d_nstransform_matrix_forward(w, a, work); }
__attribute__((export_name("lind_gsl_wavelet2d_nstransform_matrix_inverse")))
int lind_gsl_wavelet2d_nstransform_matrix_inverse(gsl_wavelet *w, struct gsl_matrix *a, gsl_wavelet_workspace *work) { return gsl_wavelet2d_nstransform_matrix_inverse(w, a, work); }

// ---------------------------------------------------------------------------
// Stage I: quasi-random (low-discrepancy) sequences (gsl_qrng). Type-pointer constructor +
// handle; gsl_qrng_get writes `dimension` doubles, where dimension is bound at alloc (the stub
// remembers it per generator to size the output). No callbacks, no engine change.
// ---------------------------------------------------------------------------
typedef struct gsl_qrng_type_s gsl_qrng_type;
typedef struct gsl_qrng_s      gsl_qrng;

extern const gsl_qrng_type *gsl_qrng_niederreiter_2;
extern const gsl_qrng_type *gsl_qrng_sobol;
extern const gsl_qrng_type *gsl_qrng_halton;
extern const gsl_qrng_type *gsl_qrng_reversehalton;
extern gsl_qrng *gsl_qrng_alloc(const gsl_qrng_type *T, unsigned int d);
extern void gsl_qrng_free(gsl_qrng *q);
extern void gsl_qrng_init(gsl_qrng *q);
extern int  gsl_qrng_get(const gsl_qrng *q, double x[]);
extern int  gsl_qrng_memcpy(gsl_qrng *dest, const gsl_qrng *src);

__attribute__((export_name("lind_gsl_qrng_alloc")))
gsl_qrng *lind_gsl_qrng_alloc(int idx, unsigned int d) {
    const gsl_qrng_type *tbl[] = { 0, gsl_qrng_niederreiter_2, gsl_qrng_sobol, gsl_qrng_halton, gsl_qrng_reversehalton };
    if (idx < 1 || idx > 4) return 0;
    return gsl_qrng_alloc(tbl[idx], d);
}
__attribute__((export_name("lind_gsl_qrng_free")))
void lind_gsl_qrng_free(gsl_qrng *q) { gsl_qrng_free(q); }
__attribute__((export_name("lind_gsl_qrng_init")))
void lind_gsl_qrng_init(gsl_qrng *q) { gsl_qrng_init(q); }
__attribute__((export_name("lind_gsl_qrng_get")))
int lind_gsl_qrng_get(gsl_qrng *q, double x[]) { return gsl_qrng_get(q, x); }
__attribute__((export_name("lind_gsl_qrng_memcpy")))
int lind_gsl_qrng_memcpy(gsl_qrng *dest, gsl_qrng *src) { return gsl_qrng_memcpy(dest, src); }

// ---------------------------------------------------------------------------
// Tier-1 tail: hand-written forwarders for the irregular flat functions. Placed AFTER
// sf_result_shim.inc.c so gsl_sf_result (used by the *_sgn_e forms) is already defined.
// ---------------------------------------------------------------------------
extern int    gsl_poly_dd_init(double dd[], const double xa[], const double ya[], size_t size);
extern double gsl_poly_dd_eval(const double dd[], const double xa[], size_t size, double x);
extern int    gsl_poly_dd_taylor(double c[], double xp, const double dd[], const double xa[], size_t size, double w[]);
extern int    gsl_poly_dd_hermite_init(double dd[], double z[], const double xa[], const double ya[], const double dya[], size_t size);
extern double gsl_ran_dirichlet_pdf(size_t K, const double alpha[], const double theta[]);
extern double gsl_ran_dirichlet_lnpdf(size_t K, const double alpha[], const double theta[]);
extern double gsl_ran_multinomial_pdf(size_t K, const double p[], const unsigned int n[]);
extern double gsl_ran_multinomial_lnpdf(size_t K, const double p[], const unsigned int n[]);
extern int    gsl_sf_lngamma_sgn_e(double x, gsl_sf_result *result, double *sgn);
extern int    gsl_sf_lnpoch_sgn_e(double a, double x, gsl_sf_result *result, double *sgn);
extern int    gsl_sf_lnbeta_sgn_e(double x, double y, gsl_sf_result *result, double *sgn);

__attribute__((export_name("lind_gsl_poly_dd_init")))
int lind_gsl_poly_dd_init(double dd[], const double xa[], const double ya[], size_t size) { return gsl_poly_dd_init(dd, xa, ya, size); }
__attribute__((export_name("lind_gsl_poly_dd_eval")))
double lind_gsl_poly_dd_eval(const double dd[], const double xa[], size_t size, double x) { return gsl_poly_dd_eval(dd, xa, size, x); }
__attribute__((export_name("lind_gsl_poly_dd_taylor")))
int lind_gsl_poly_dd_taylor(double c[], double xp, const double dd[], const double xa[], size_t size, double w[]) { return gsl_poly_dd_taylor(c, xp, dd, xa, size, w); }
__attribute__((export_name("lind_gsl_poly_dd_hermite_init")))
int lind_gsl_poly_dd_hermite_init(double dd[], double z[], const double xa[], const double ya[], const double dya[], size_t size) { return gsl_poly_dd_hermite_init(dd, z, xa, ya, dya, size); }
__attribute__((export_name("lind_gsl_ran_dirichlet_pdf")))
double lind_gsl_ran_dirichlet_pdf(size_t K, const double alpha[], const double theta[]) { return gsl_ran_dirichlet_pdf(K, alpha, theta); }
__attribute__((export_name("lind_gsl_ran_dirichlet_lnpdf")))
double lind_gsl_ran_dirichlet_lnpdf(size_t K, const double alpha[], const double theta[]) { return gsl_ran_dirichlet_lnpdf(K, alpha, theta); }
__attribute__((export_name("lind_gsl_ran_multinomial_pdf")))
double lind_gsl_ran_multinomial_pdf(size_t K, const double p[], const unsigned int n[]) { return gsl_ran_multinomial_pdf(K, p, n); }
__attribute__((export_name("lind_gsl_ran_multinomial_lnpdf")))
double lind_gsl_ran_multinomial_lnpdf(size_t K, const double p[], const unsigned int n[]) { return gsl_ran_multinomial_lnpdf(K, p, n); }
__attribute__((export_name("lind_gsl_sf_lngamma_sgn_e")))
int lind_gsl_sf_lngamma_sgn_e(double x, gsl_sf_result *result, double *sgn) { return gsl_sf_lngamma_sgn_e(x, result, sgn); }
__attribute__((export_name("lind_gsl_sf_lnpoch_sgn_e")))
int lind_gsl_sf_lnpoch_sgn_e(double a, double x, gsl_sf_result *result, double *sgn) { return gsl_sf_lnpoch_sgn_e(a, x, result, sgn); }
__attribute__((export_name("lind_gsl_sf_lnbeta_sgn_e")))
int lind_gsl_sf_lnbeta_sgn_e(double x, double y, gsl_sf_result *result, double *sgn) { return gsl_sf_lnbeta_sgn_e(x, y, result, sgn); }
