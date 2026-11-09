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

// Opaque handle type: forward-declared (incomplete) so it can only ever be passed by
// pointer, never dereferenced here. gsl_vector_get/set are inline in the GSL headers, so
// we deliberately do NOT #include <gsl/...>; forward-declaring keeps them as dynamic
// imports resolved from the preloaded GSL (same reason as the poly functions above).
struct gsl_vector;
extern struct gsl_vector *gsl_vector_alloc(size_t n);
extern void   gsl_vector_set(struct gsl_vector *v, size_t i, double x);
extern double gsl_vector_get(const struct gsl_vector *v, size_t i);
extern void   gsl_sort_vector(struct gsl_vector *v);
extern void   gsl_vector_free(struct gsl_vector *v);

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

// GSL opaque-handle forwarders (gsl_vector). Each just passes the guest pointer through
// to the real preloaded GSL; the host side (Arg::Handle) does the token<->offset mapping.
__attribute__((export_name("lind_gsl_vector_alloc")))
struct gsl_vector *lind_gsl_vector_alloc(size_t n) { return gsl_vector_alloc(n); }

__attribute__((export_name("lind_gsl_vector_set")))
void lind_gsl_vector_set(struct gsl_vector *v, size_t i, double x) { gsl_vector_set(v, i, x); }

__attribute__((export_name("lind_gsl_vector_get")))
double lind_gsl_vector_get(const struct gsl_vector *v, size_t i) { return gsl_vector_get(v, i); }

__attribute__((export_name("lind_gsl_sort_vector")))
void lind_gsl_sort_vector(struct gsl_vector *v) { gsl_sort_vector(v); }

__attribute__((export_name("lind_gsl_vector_free")))
void lind_gsl_vector_free(struct gsl_vector *v) { gsl_vector_free(v); }

// GSL sort module — generated forwarders (see gen_gsl_sort.py).
#include "generated/sort_shim.inc.c"

// GSL statistics module — generated forwarders (see gen_gsl_stats.py).
#include "generated/stats_shim.inc.c"

// GSL scalar modules (cdf / randist pdf / sys) — generated forwarders (see gen_gsl_scalar.py).
#include "generated/scalar_shim.inc.c"

// GSL special-function `_e` forms (gsl_sf_*_e) — generated forwarders (see gen_gsl_sf_result.py).
#include "generated/sf_result_shim.inc.c"

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
