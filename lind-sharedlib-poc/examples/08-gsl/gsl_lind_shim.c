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
