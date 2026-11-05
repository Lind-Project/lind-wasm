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
