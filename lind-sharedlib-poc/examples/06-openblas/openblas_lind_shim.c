// The resident SHIM — the sandbox's MAIN module (Option A).
//
// A `-shared` OpenBLAS can't be the main module (its internal function tables only get
// set up on the preload path), so OpenBLAS is loaded as a PRELOAD and this small module
// is the main module instead. Two jobs:
//   1. Export guest_malloc/guest_free so the host (SandboxedLib) can place matrices and
//      vectors into the shared linear memory and take results back out.
//   2. Export a thin wrapper per BLAS function under a stable name (lind_cblas_*). The
//      wrappers call cblas_*, which are UNDEFINED here — the default (dynamic)
//      lind_compile build turns them into dynamic imports, resolved at runtime from the
//      preloaded OpenBLAS (and malloc/free from libc). So the host only ever looks up
//      the lind_cblas_*/guest_* symbols this module defines.
//
// Build:  lind_compile openblas_lind_shim.c   (see the Makefile `shim` target)
// Run:    LIND_MODULE = this .cwasm;  LIND_PRELOAD includes plain OpenBLAS + libc/libm.
//
// (The same source can alternatively be linked INTO OpenBLAS via --whole-archive, which
// defines cblas_* statically — but that path loads OpenBLAS as main and traps on
// indirect kernel dispatch. Option A, above, is the working model.)

#include <stddef.h>
#include <stdlib.h>

// We deliberately do NOT include <cblas.h>: it pulls in OpenBLAS's internal common.h
// and a generated config.h that only exists in the build tree. Forward-declare the
// functions we wrap instead; their definitions come from libopenblas.a at link time.
// These prototypes match OpenBLAS's cblas.h for a 32-bit-index (BINARY=32) build.
extern size_t cblas_idamax(const int, const double *, const int);
extern double cblas_ddot (const int, const double *, const int, const double *, const int);
extern double cblas_dnrm2(const int, const double *, const int);
extern double cblas_dasum(const int, const double *, const int);
extern void   cblas_daxpy(const int, const double, const double *, const int, double *, const int);
extern void   cblas_dcopy(const int, const double *, const int, double *, const int);
extern void   cblas_dswap(const int, double *, const int, double *, const int);
extern void   cblas_dscal(const int, const double, double *, const int);
extern void   cblas_drot (const int, double *, const int, double *, const int, const double, const double);
extern void   cblas_drotg(double *, double *, double *, double *);
extern void   cblas_drotm(const int, double *, const int, double *, const int, const double *);

extern size_t cblas_isamax(const int, const float *, const int);
extern float  cblas_sdot (const int, const float *, const int, const float *, const int);
extern float  cblas_snrm2(const int, const float *, const int);
extern float  cblas_sasum(const int, const float *, const int);
extern void   cblas_saxpy(const int, const float, const float *, const int, float *, const int);
extern void   cblas_scopy(const int, const float *, const int, float *, const int);
extern void   cblas_sswap(const int, float *, const int, float *, const int);
extern void   cblas_sscal(const int, const float, float *, const int);
extern void   cblas_srot (const int, float *, const int, float *, const int, const float, const float);
extern void   cblas_srotg(float *, float *, float *, float *);
extern void   cblas_srotm(const int, float *, const int, float *, const int, const float *);

// complex level-1 (c = single-complex, z = double-complex). Complex scalars/vectors
// cross as void*; scalars alpha are by-pointer; dot returns via an out-pointer (?dot?_sub);
// the norm/asum reductions return a real (float / double).
extern size_t cblas_icamax(const int, const void *, const int);
extern float  cblas_scnrm2(const int, const void *, const int);
extern float  cblas_scasum(const int, const void *, const int);
extern void   cblas_cdotu_sub(const int, const void *, const int, const void *, const int, void *);
extern void   cblas_cdotc_sub(const int, const void *, const int, const void *, const int, void *);
extern void   cblas_caxpy(const int, const void *, const void *, const int, void *, const int);
extern void   cblas_ccopy(const int, const void *, const int, void *, const int);
extern void   cblas_cswap(const int, void *, const int, void *, const int);
extern void   cblas_cscal(const int, const void *, void *, const int);
extern void   cblas_csscal(const int, const float, void *, const int);
extern size_t cblas_izamax(const int, const void *, const int);
extern double cblas_dznrm2(const int, const void *, const int);
extern double cblas_dzasum(const int, const void *, const int);
extern void   cblas_zdotu_sub(const int, const void *, const int, const void *, const int, void *);
extern void   cblas_zdotc_sub(const int, const void *, const int, const void *, const int, void *);
extern void   cblas_zaxpy(const int, const void *, const void *, const int, void *, const int);
extern void   cblas_zcopy(const int, const void *, const int, void *, const int);
extern void   cblas_zswap(const int, void *, const int, void *, const int);
extern void   cblas_zscal(const int, const void *, void *, const int);
extern void   cblas_zdscal(const int, const double, void *, const int);

// complex level-2. alpha/beta by pointer (void*) for gemv/gbmv/hemv/hbmv/hpmv/geru/gerc/
// her2/hpr2; her/hpr take a REAL alpha by value; ger splits into geru/gerc.
extern void cblas_cgemv(const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_cgbmv(const int, const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_chemv(const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_chbmv(const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_chpmv(const int, const int, const int, const void *, const void *, const void *, const int, const void *, void *, const int);
extern void cblas_ctrmv(const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ctbmv(const int, const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ctpmv(const int, const int, const int, const int, const int, const void *, void *, const int);
extern void cblas_ctrsv(const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ctbsv(const int, const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ctpsv(const int, const int, const int, const int, const int, const void *, void *, const int);
extern void cblas_cgeru(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *, const int);
extern void cblas_cgerc(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *, const int);
extern void cblas_cher (const int, const int, const int, const float, const void *, const int, void *, const int);
extern void cblas_chpr (const int, const int, const int, const float, const void *, const int, void *);
extern void cblas_cher2(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *, const int);
extern void cblas_chpr2(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *);
extern void cblas_zgemv(const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zgbmv(const int, const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zhemv(const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zhbmv(const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zhpmv(const int, const int, const int, const void *, const void *, const void *, const int, const void *, void *, const int);
extern void cblas_ztrmv(const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ztbmv(const int, const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ztpmv(const int, const int, const int, const int, const int, const void *, void *, const int);
extern void cblas_ztrsv(const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ztbsv(const int, const int, const int, const int, const int, const int, const void *, const int, void *, const int);
extern void cblas_ztpsv(const int, const int, const int, const int, const int, const void *, void *, const int);
extern void cblas_zgeru(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *, const int);
extern void cblas_zgerc(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *, const int);
extern void cblas_zher (const int, const int, const int, const double, const void *, const int, void *, const int);
extern void cblas_zhpr (const int, const int, const int, const double, const void *, const int, void *);
extern void cblas_zher2(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *, const int);
extern void cblas_zhpr2(const int, const int, const int, const void *, const void *, const int, const void *, const int, void *);

// complex level-3. alpha/beta by pointer for gemm/symm/hemm/syrk/syr2k (+ alpha for
// trmm/trsm); herk takes REAL alpha AND beta by value; her2k takes complex alpha (ptr)
// but REAL beta by value.
extern void cblas_cgemm (const int, const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_csymm (const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_chemm (const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_csyrk (const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, void *, const int);
extern void cblas_cherk (const int, const int, const int, const int, const int, const float, const void *, const int, const float, void *, const int);
extern void cblas_csyr2k(const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_cher2k(const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const float, void *, const int);
extern void cblas_ctrmm (const int, const int, const int, const int, const int, const int, const int, const void *, const void *, const int, void *, const int);
extern void cblas_ctrsm (const int, const int, const int, const int, const int, const int, const int, const void *, const void *, const int, void *, const int);
extern void cblas_zgemm (const int, const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zsymm (const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zhemm (const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zsyrk (const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, void *, const int);
extern void cblas_zherk (const int, const int, const int, const int, const int, const double, const void *, const int, const double, void *, const int);
extern void cblas_zsyr2k(const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const void *, void *, const int);
extern void cblas_zher2k(const int, const int, const int, const int, const int, const void *, const void *, const int, const void *, const int, const double, void *, const int);
extern void cblas_ztrmm (const int, const int, const int, const int, const int, const int, const int, const void *, const void *, const int, void *, const int);
extern void cblas_ztrsm (const int, const int, const int, const int, const int, const int, const int, const void *, const void *, const int, void *, const int);


// level-2 (order/trans are enums, ABI-compatible with int)
extern void   cblas_dgemv(const int, const int, const int, const int, const double,
                          const double *, const int, const double *, const int,
                          const double, double *, const int);
extern void   cblas_sgemv(const int, const int, const int, const int, const float,
                          const float *, const int, const float *, const int,
                          const float, float *, const int);

// level-2 (order/trans are enums, ABI-compatible with int)
extern void   cblas_dgemv(const int, const int, const int, const int, const double,
                          const double *, const int, const double *, const int,
                          const double, double *, const int);
extern void   cblas_sgemv(const int, const int, const int, const int, const float,
                          const float *, const int, const float *, const int,
                          const float, float *, const int);

// level-2 general/square group
extern void cblas_dsymv(const int, const int, const int, const double, const double *, const int, const double *, const int, const double, double *, const int);
extern void cblas_dtrmv(const int, const int, const int, const int, const int, const double *, const int, double *, const int);
extern void cblas_dtrsv(const int, const int, const int, const int, const int, const double *, const int, double *, const int);
extern void cblas_dger (const int, const int, const int, const double, const double *, const int, const double *, const int, double *, const int);
extern void cblas_dsyr (const int, const int, const int, const double, const double *, const int, double *, const int);
extern void cblas_dsyr2(const int, const int, const int, const double, const double *, const int, const double *, const int, double *, const int);
extern void cblas_ssymv(const int, const int, const int, const float, const float *, const int, const float *, const int, const float, float *, const int);
extern void cblas_strmv(const int, const int, const int, const int, const int, const float *, const int, float *, const int);
extern void cblas_strsv(const int, const int, const int, const int, const int, const float *, const int, float *, const int);
extern void cblas_sger (const int, const int, const int, const float, const float *, const int, const float *, const int, float *, const int);
extern void cblas_ssyr (const int, const int, const int, const float, const float *, const int, float *, const int);
extern void cblas_ssyr2(const int, const int, const int, const float, const float *, const int, const float *, const int, float *, const int);

// level-2 banded + packed
extern void cblas_dgbmv(const int, const int, const int, const int, const int, const int, const double, const double *, const int, const double *, const int, const double, double *, const int);
extern void cblas_dsbmv(const int, const int, const int, const int, const double, const double *, const int, const double *, const int, const double, double *, const int);
extern void cblas_dtbmv(const int, const int, const int, const int, const int, const int, const double *, const int, double *, const int);
extern void cblas_dtbsv(const int, const int, const int, const int, const int, const int, const double *, const int, double *, const int);
extern void cblas_dspmv(const int, const int, const int, const double, const double *, const double *, const int, const double, double *, const int);
extern void cblas_dspr (const int, const int, const int, const double, const double *, const int, double *);
extern void cblas_dtpmv(const int, const int, const int, const int, const int, const double *, double *, const int);
extern void cblas_dtpsv(const int, const int, const int, const int, const int, const double *, double *, const int);
extern void cblas_dspr2(const int, const int, const int, const double, const double *, const int, const double *, const int, double *);
extern void cblas_sgbmv(const int, const int, const int, const int, const int, const int, const float, const float *, const int, const float *, const int, const float, float *, const int);
extern void cblas_ssbmv(const int, const int, const int, const int, const float, const float *, const int, const float *, const int, const float, float *, const int);
extern void cblas_stbmv(const int, const int, const int, const int, const int, const int, const float *, const int, float *, const int);
extern void cblas_stbsv(const int, const int, const int, const int, const int, const int, const float *, const int, float *, const int);
extern void cblas_sspmv(const int, const int, const int, const float, const float *, const float *, const int, const float, float *, const int);
extern void cblas_sspr (const int, const int, const int, const float, const float *, const int, float *);
extern void cblas_stpmv(const int, const int, const int, const int, const int, const float *, float *, const int);
extern void cblas_stpsv(const int, const int, const int, const int, const int, const float *, float *, const int);
extern void cblas_sspr2(const int, const int, const int, const float, const float *, const int, const float *, const int, float *);

// level-3
extern void cblas_dgemm (const int, const int, const int, const int, const int, const int, const double, const double *, const int, const double *, const int, const double, double *, const int);
extern void cblas_dsymm (const int, const int, const int, const int, const int, const double, const double *, const int, const double *, const int, const double, double *, const int);
extern void cblas_dsyrk (const int, const int, const int, const int, const int, const double, const double *, const int, const double, double *, const int);
extern void cblas_dsyr2k(const int, const int, const int, const int, const int, const double, const double *, const int, const double *, const int, const double, double *, const int);
extern void cblas_dtrmm (const int, const int, const int, const int, const int, const int, const int, const double, const double *, const int, double *, const int);
extern void cblas_dtrsm (const int, const int, const int, const int, const int, const int, const int, const double, const double *, const int, double *, const int);
extern void cblas_sgemm (const int, const int, const int, const int, const int, const int, const float, const float *, const int, const float *, const int, const float, float *, const int);
extern void cblas_ssymm (const int, const int, const int, const int, const int, const float, const float *, const int, const float *, const int, const float, float *, const int);
extern void cblas_ssyrk (const int, const int, const int, const int, const int, const float, const float *, const int, const float, float *, const int);
extern void cblas_ssyr2k(const int, const int, const int, const int, const int, const float, const float *, const int, const float *, const int, const float, float *, const int);
extern void cblas_strmm (const int, const int, const int, const int, const int, const int, const int, const float, const float *, const int, float *, const int);
extern void cblas_strsm (const int, const int, const int, const int, const int, const int, const int, const float, const float *, const int, float *, const int);


__attribute__((export_name("guest_malloc")))
void *guest_malloc(size_t n) { return malloc(n); }

__attribute__((export_name("guest_free")))
void guest_free(void *p) { free(p); }

// --- BLAS level-1 (double) subset -------------------------------------------------

__attribute__((export_name("lind_cblas_idamax")))
size_t lind_cblas_idamax(int n, const double *x, int incx) {
    return cblas_idamax(n, x, incx);
}

__attribute__((export_name("lind_cblas_ddot")))
double lind_cblas_ddot(int n, const double *x, int incx, const double *y, int incy) {
    return cblas_ddot(n, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_dnrm2")))
double lind_cblas_dnrm2(int n, const double *x, int incx) {
    return cblas_dnrm2(n, x, incx);
}

__attribute__((export_name("lind_cblas_dasum")))
double lind_cblas_dasum(int n, const double *x, int incx) {
    return cblas_dasum(n, x, incx);
}

__attribute__((export_name("lind_cblas_daxpy")))
void lind_cblas_daxpy(int n, double alpha, const double *x, int incx,
                      double *y, int incy) {
    cblas_daxpy(n, alpha, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_dcopy")))
void lind_cblas_dcopy(int n, const double *x, int incx, double *y, int incy) {
    cblas_dcopy(n, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_dswap")))
void lind_cblas_dswap(int n, double *x, int incx, double *y, int incy) {
    cblas_dswap(n, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_dscal")))
void lind_cblas_dscal(int n, double alpha, double *x, int incx) {
    cblas_dscal(n, alpha, x, incx);
}

__attribute__((export_name("lind_cblas_drot")))
void lind_cblas_drot(int n, double *x, int incx, double *y, int incy,
                     double c, double s) {
    cblas_drot(n, x, incx, y, incy, c, s);
}

__attribute__((export_name("lind_cblas_drotg")))
void lind_cblas_drotg(double *a, double *b, double *c, double *s) {
    cblas_drotg(a, b, c, s);
}

__attribute__((export_name("lind_cblas_drotm")))
void lind_cblas_drotm(int n, double *x, int incx, double *y, int incy,
                      const double *param) {
    cblas_drotm(n, x, incx, y, incy, param);
}

// --- BLAS level-1 (single) subset -------------------------------------------------

__attribute__((export_name("lind_cblas_isamax")))
size_t lind_cblas_isamax(int n, const float *x, int incx) {
    return cblas_isamax(n, x, incx);
}

__attribute__((export_name("lind_cblas_sdot")))
float lind_cblas_sdot(int n, const float *x, int incx, const float *y, int incy) {
    return cblas_sdot(n, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_snrm2")))
float lind_cblas_snrm2(int n, const float *x, int incx) {
    return cblas_snrm2(n, x, incx);
}

__attribute__((export_name("lind_cblas_sasum")))
float lind_cblas_sasum(int n, const float *x, int incx) {
    return cblas_sasum(n, x, incx);
}

__attribute__((export_name("lind_cblas_saxpy")))
void lind_cblas_saxpy(int n, float alpha, const float *x, int incx,
                      float *y, int incy) {
    cblas_saxpy(n, alpha, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_scopy")))
void lind_cblas_scopy(int n, const float *x, int incx, float *y, int incy) {
    cblas_scopy(n, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_sswap")))
void lind_cblas_sswap(int n, float *x, int incx, float *y, int incy) {
    cblas_sswap(n, x, incx, y, incy);
}

__attribute__((export_name("lind_cblas_sscal")))
void lind_cblas_sscal(int n, float alpha, float *x, int incx) {
    cblas_sscal(n, alpha, x, incx);
}

__attribute__((export_name("lind_cblas_srot")))
void lind_cblas_srot(int n, float *x, int incx, float *y, int incy,
                     float c, float s) {
    cblas_srot(n, x, incx, y, incy, c, s);
}

__attribute__((export_name("lind_cblas_srotg")))
void lind_cblas_srotg(float *a, float *b, float *c, float *s) {
    cblas_srotg(a, b, c, s);
}

__attribute__((export_name("lind_cblas_srotm")))
void lind_cblas_srotm(int n, float *x, int incx, float *y, int incy,
                      const float *param) {
    cblas_srotm(n, x, incx, y, incy, param);
}

// --- BLAS level-2 subset ----------------------------------------------------------

__attribute__((export_name("lind_cblas_dgemv")))
void lind_cblas_dgemv(int order, int trans, int m, int n, double alpha,
                      const double *a, int lda, const double *x, int incx,
                      double beta, double *y, int incy) {
    cblas_dgemv(order, trans, m, n, alpha, a, lda, x, incx, beta, y, incy);
}

__attribute__((export_name("lind_cblas_sgemv")))
void lind_cblas_sgemv(int order, int trans, int m, int n, float alpha,
                      const float *a, int lda, const float *x, int incx,
                      float beta, float *y, int incy) {
    cblas_sgemv(order, trans, m, n, alpha, a, lda, x, incx, beta, y, incy);
}


// level-2 general/square group
__attribute__((export_name("lind_cblas_dsymv")))
void lind_cblas_dsymv(int o,int u,int n,double al,const double*a,int lda,const double*x,
int ix,double be,double*y,int iy){ cblas_dsymv(o,u,n,al,a,lda,x,ix,be,y,iy); }__attribute__((export_name("lind_cblas_dtrmv")))
void lind_cblas_dtrmv(int o,int u,int t,int d,int n,const double*a,int lda,double*x,int ix){ cblas_dtrmv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_dtrsv")))
void lind_cblas_dtrsv(int o,int u,int t,int d,int n,const double*a,int lda,double*x,int ix){ cblas_dtrsv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_dger")))
void lind_cblas_dger(int o,int m,int n,double al,const double*x,int ix,const double*y,int iy,double*a,int lda){ cblas_dger(o,m,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_dsyr")))
void lind_cblas_dsyr(int o,int u,int n,double al,const double*x,int ix,double*a,int lda)
{ cblas_dsyr(o,u,n,al,x,ix,a,lda); }
__attribute__((export_name("lind_cblas_dsyr2")))
void lind_cblas_dsyr2(int o,int u,int n,double al,const double*x,int ix,const double*y,int iy,double*a,int lda){ cblas_dsyr2(o,u,n,al,x,ix,y,iy,a,lda); }

__attribute__((export_name("lind_cblas_ssymv")))
void lind_cblas_ssymv(int o,int u,int n,float al,const float*a,int lda,const float*x,int ix,float be,float*y,int iy){ cblas_ssymv(o,u,n,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_strmv")))
void lind_cblas_strmv(int o,int u,int t,int d,int n,const float*a,int lda,float*x,int ix){ cblas_strmv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_strsv")))
void lind_cblas_strsv(int o,int u,int t,int d,int n,const float*a,int lda,float*x,int ix){ cblas_strsv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_sger")))
void lind_cblas_sger(int o,int m,int n,float al,const float*x,int ix,const float*y,int iy,float*a,int lda){ cblas_sger(o,m,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_ssyr")))
void lind_cblas_ssyr(int o,int u,int n,float al,const float*x,int ix,float*a,int lda){ cblas_ssyr(o,u,n,al,x,ix,a,lda); }
__attribute__((export_name("lind_cblas_ssyr2")))
void lind_cblas_ssyr2(int o,int u,int n,float al,const float*x,int ix,const float*y,int iy,float*a,int lda){ cblas_ssyr2(o,u,n,al,x,ix,y,iy,a,lda); }


// level-2 banded + packed
__attribute__((export_name("lind_cblas_dgbmv")))
void lind_cblas_dgbmv(int o,int t,int m,int n,int kl,int ku,double al,const double*a,int lda,const double*x,int ix,double be,double*y,int iy){ cblas_dgbmv(o,t,m,n,kl,ku,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_dsbmv")))
void lind_cblas_dsbmv(int o,int u,int n,int k,double al,const double*a,int lda,const double*x,int ix,double be,double*y,int iy){ cblas_dsbmv(o,u,n,k,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_dtbmv")))
void lind_cblas_dtbmv(int o,int u,int t,int d,int n,int k,const double*a,int lda,double*x,int ix){ cblas_dtbmv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_dtbsv")))
void lind_cblas_dtbsv(int o,int u,int t,int d,int n,int k,const double*a,int lda,double*x,int ix){ cblas_dtbsv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_dspmv")))
void lind_cblas_dspmv(int o,int u,int n,double al,const double*ap,const double*x,int ix,double be,double*y,int iy){ cblas_dspmv(o,u,n,al,ap,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_dspr")))
void lind_cblas_dspr(int o,int u,int n,double al,const double*x,int ix,double*ap){ cblas_dspr(o,u,n,al,x,ix,ap); }
__attribute__((export_name("lind_cblas_dtpmv")))
void lind_cblas_dtpmv(int o,int u,int t,int d,int n,const double*ap,double*x,int ix){ cblas_dtpmv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_dtpsv")))
void lind_cblas_dtpsv(int o,int u,int t,int d,int n,const double*ap,double*x,int ix){ cblas_dtpsv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_dspr2")))
void lind_cblas_dspr2(int o,int u,int n,double al,const double*x,int ix,const double*y,int iy,double*ap){ cblas_dspr2(o,u,n,al,x,ix,y,iy,ap); }

__attribute__((export_name("lind_cblas_sgbmv")))
void lind_cblas_sgbmv(int o,int t,int m,int n,int kl,int ku,float al,const float*a,int lda,const float*x,int ix,float be,float*y,int iy){ cblas_sgbmv(o,t,m,n,kl,ku,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_ssbmv")))
void lind_cblas_ssbmv(int o,int u,int n,int k,float al,const float*a,int lda,const float*x,int ix,float be,float*y,int iy){ cblas_ssbmv(o,u,n,k,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_stbmv")))
void lind_cblas_stbmv(int o,int u,int t,int d,int n,int k,const float*a,int lda,float*x,int ix){ cblas_stbmv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_stbsv")))
void lind_cblas_stbsv(int o,int u,int t,int d,int n,int k,const float*a,int lda,float*x,int ix){ cblas_stbsv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_sspmv")))
void lind_cblas_sspmv(int o,int u,int n,float al,const float*ap,const float*x,int ix,float be,float*y,int iy){ cblas_sspmv(o,u,n,al,ap,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_sspr")))
void lind_cblas_sspr(int o,int u,int n,float al,const float*x,int ix,float*ap){ cblas_sspr(o,u,n,al,x,ix,ap); }
__attribute__((export_name("lind_cblas_stpmv")))
void lind_cblas_stpmv(int o,int u,int t,int d,int n,const float*ap,float*x,int ix){ cblas_stpmv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_stpsv")))
void lind_cblas_stpsv(int o,int u,int t,int d,int n,const float*ap,float*x,int ix){ cblas_stpsv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_sspr2")))
void lind_cblas_sspr2(int o,int u,int n,float al,const float*x,int ix,const float*y,int iy,float*ap){ cblas_sspr2(o,u,n,al,x,ix,y,iy,ap); }

// level-3
__attribute__((export_name("lind_cblas_dgemm")))
void lind_cblas_dgemm(int o,int ta,int tb,int m,int n,int k,double al,const double*a,int lda,const double*b,int ldb,double be,double*c,int ldc){ cblas_dgemm(o,ta,tb,m,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_dsymm")))
void lind_cblas_dsymm(int o,int s,int u,int m,int n,double al,const double*a,int lda,const double*b,int ldb,double be,double*c,int ldc){ cblas_dsymm(o,s,u,m,n,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_dsyrk")))
void lind_cblas_dsyrk(int o,int u,int t,int n,int k,double al,const double*a,int lda,double be,double*c,int ldc){ cblas_dsyrk(o,u,t,n,k,al,a,lda,be,c,ldc); }
__attribute__((export_name("lind_cblas_dsyr2k")))
void lind_cblas_dsyr2k(int o,int u,int t,int n,int k,double al,const double*a,int lda,const double*b,int ldb,double be,double*c,int ldc){ cblas_dsyr2k(o,u,t,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_dtrmm")))
void lind_cblas_dtrmm(int o,int s,int u,int t,int d,int m,int n,double al,const double*a,int lda,double*b,int ldb){ cblas_dtrmm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }
__attribute__((export_name("lind_cblas_dtrsm")))
void lind_cblas_dtrsm(int o,int s,int u,int t,int d,int m,int n,double al,const double*a,int lda,double*b,int ldb){ cblas_dtrsm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }

__attribute__((export_name("lind_cblas_sgemm")))
void lind_cblas_sgemm(int o,int ta,int tb,int m,int n,int k,float al,const float*a,int lda,const float*b,int ldb,float be,float*c,int ldc){ cblas_sgemm(o,ta,tb,m,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_ssymm")))
void lind_cblas_ssymm(int o,int s,int u,int m,int n,float al,const float*a,int lda,const float*b,int ldb,float be,float*c,int ldc){ cblas_ssymm(o,s,u,m,n,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_ssyrk")))
void lind_cblas_ssyrk(int o,int u,int t,int n,int k,float al,const float*a,int lda,float be,float*c,int ldc){ cblas_ssyrk(o,u,t,n,k,al,a,lda,be,c,ldc); }
__attribute__((export_name("lind_cblas_ssyr2k")))
void lind_cblas_ssyr2k(int o,int u,int t,int n,int k,float al,const float*a,int lda,const float*b,int ldb,float be,float*c,int ldc){ cblas_ssyr2k(o,u,t,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_strmm")))
void lind_cblas_strmm(int o,int s,int u,int t,int d,int m,int n,float al,const float*a,int lda,float*b,int ldb){ cblas_strmm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }
__attribute__((export_name("lind_cblas_strsm")))
void lind_cblas_strsm(int o,int s,int u,int t,int d,int m,int n,float al,const float*a,int lda,float*b,int ldb){ cblas_strsm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }

// --- complex level-1 --------------------------------------------------------------
__attribute__((export_name("lind_cblas_icamax")))
size_t lind_cblas_icamax(int n,const void*x,int ix){ return cblas_icamax(n,x,ix); }
__attribute__((export_name("lind_cblas_scnrm2")))
float lind_cblas_scnrm2(int n,const void*x,int ix){ return cblas_scnrm2(n,x,ix); }
__attribute__((export_name("lind_cblas_scasum")))
float lind_cblas_scasum(int n,const void*x,int ix){ return cblas_scasum(n,x,ix); }
__attribute__((export_name("lind_cblas_cdotu_sub")))
void lind_cblas_cdotu_sub(int n,const void*x,int ix,const void*y,int iy,void*d){ cblas_cdotu_sub(n,x,ix,y,iy,d); }
__attribute__((export_name("lind_cblas_cdotc_sub")))
void lind_cblas_cdotc_sub(int n,const void*x,int ix,const void*y,int iy,void*d){ cblas_cdotc_sub(n,x,ix,y,iy,d); }
__attribute__((export_name("lind_cblas_caxpy")))
void lind_cblas_caxpy(int n,const void*al,const void*x,int ix,void*y,int iy){ cblas_caxpy(n,al,x,ix,y,iy); }
__attribute__((export_name("lind_cblas_ccopy")))
void lind_cblas_ccopy(int n,const void*x,int ix,void*y,int iy){ cblas_ccopy(n,x,ix,y,iy); }
__attribute__((export_name("lind_cblas_cswap")))
void lind_cblas_cswap(int n,void*x,int ix,void*y,int iy){ cblas_cswap(n,x,ix,y,iy); }
__attribute__((export_name("lind_cblas_cscal")))
void lind_cblas_cscal(int n,const void*al,void*x,int ix){ cblas_cscal(n,al,x,ix); }
__attribute__((export_name("lind_cblas_csscal")))
void lind_cblas_csscal(int n,float al,void*x,int ix){ cblas_csscal(n,al,x,ix); }

__attribute__((export_name("lind_cblas_izamax")))
size_t lind_cblas_izamax(int n,const void*x,int ix){ return cblas_izamax(n,x,ix); }
__attribute__((export_name("lind_cblas_dznrm2")))
double lind_cblas_dznrm2(int n,const void*x,int ix){ return cblas_dznrm2(n,x,ix); }
__attribute__((export_name("lind_cblas_dzasum")))
double lind_cblas_dzasum(int n,const void*x,int ix){ return cblas_dzasum(n,x,ix); }
__attribute__((export_name("lind_cblas_zdotu_sub")))
void lind_cblas_zdotu_sub(int n,const void*x,int ix,const void*y,int iy,void*d){ cblas_zdotu_sub(n,x,ix,y,iy,d); }
__attribute__((export_name("lind_cblas_zdotc_sub")))
void lind_cblas_zdotc_sub(int n,const void*x,int ix,const void*y,int iy,void*d){ cblas_zdotc_sub(n,x,ix,y,iy,d); }
__attribute__((export_name("lind_cblas_zaxpy")))
void lind_cblas_zaxpy(int n,const void*al,const void*x,int ix,void*y,int iy){ cblas_zaxpy(n,al,x,ix,y,iy); }
__attribute__((export_name("lind_cblas_zcopy")))
void lind_cblas_zcopy(int n,const void*x,int ix,void*y,int iy){ cblas_zcopy(n,x,ix,y,iy); }
__attribute__((export_name("lind_cblas_zswap")))
void lind_cblas_zswap(int n,void*x,int ix,void*y,int iy){ cblas_zswap(n,x,ix,y,iy); }
__attribute__((export_name("lind_cblas_zscal")))
void lind_cblas_zscal(int n,const void*al,void*x,int ix){ cblas_zscal(n,al,x,ix); }
__attribute__((export_name("lind_cblas_zdscal")))
void lind_cblas_zdscal(int n,double al,void*x,int ix){ cblas_zdscal(n,al,x,ix); }

// --- complex level-2 --------------------------------------------------------------
__attribute__((export_name("lind_cblas_cgemv")))
void lind_cblas_cgemv(int o,int t,int m,int n,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_cgemv(o,t,m,n,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_cgbmv")))
void lind_cblas_cgbmv(int o,int t,int m,int n,int kl,int ku,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_cgbmv(o,t,m,n,kl,ku,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_chemv")))
void lind_cblas_chemv(int o,int u,int n,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_chemv(o,u,n,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_chbmv")))
void lind_cblas_chbmv(int o,int u,int n,int k,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_chbmv(o,u,n,k,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_chpmv")))
void lind_cblas_chpmv(int o,int u,int n,const void*al,const void*ap,const void*x,int ix,const void*be,void*y,int iy){ cblas_chpmv(o,u,n,al,ap,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_ctrmv")))
void lind_cblas_ctrmv(int o,int u,int t,int d,int n,const void*a,int lda,void*x,int ix){ cblas_ctrmv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ctbmv")))
void lind_cblas_ctbmv(int o,int u,int t,int d,int n,int k,const void*a,int lda,void*x,int ix){ cblas_ctbmv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ctpmv")))
void lind_cblas_ctpmv(int o,int u,int t,int d,int n,const void*ap,void*x,int ix){ cblas_ctpmv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_ctrsv")))
void lind_cblas_ctrsv(int o,int u,int t,int d,int n,const void*a,int lda,void*x,int ix){ cblas_ctrsv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ctbsv")))
void lind_cblas_ctbsv(int o,int u,int t,int d,int n,int k,const void*a,int lda,void*x,int ix){ cblas_ctbsv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ctpsv")))
void lind_cblas_ctpsv(int o,int u,int t,int d,int n,const void*ap,void*x,int ix){ cblas_ctpsv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_cgeru")))
void lind_cblas_cgeru(int o,int m,int n,const void*al,const void*x,int ix,const void*y,int iy,void*a,int lda){ cblas_cgeru(o,m,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_cgerc")))
void lind_cblas_cgerc(int o,int m,int n,const void*al,const void*x,int ix,const void*y,int iy,void*a,int lda){ cblas_cgerc(o,m,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_cher")))
void lind_cblas_cher(int o,int u,int n,float al,const void*x,int ix,void*a,int lda){ cblas_cher(o,u,n,al,x,ix,a,lda); }
__attribute__((export_name("lind_cblas_chpr")))
void lind_cblas_chpr(int o,int u,int n,float al,const void*x,int ix,void*ap){ cblas_chpr(o,u,n,al,x,ix,ap); }
__attribute__((export_name("lind_cblas_cher2")))
void lind_cblas_cher2(int o,int u,int n,const void*al,const void*x,int ix,const void*y,int iy,void*a,int lda){ cblas_cher2(o,u,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_chpr2")))
void lind_cblas_chpr2(int o,int u,int n,const void*al,const void*x,int ix,const void*y,int iy,void*ap){ cblas_chpr2(o,u,n,al,x,ix,y,iy,ap); }

__attribute__((export_name("lind_cblas_zgemv")))
void lind_cblas_zgemv(int o,int t,int m,int n,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_zgemv(o,t,m,n,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_zgbmv")))
void lind_cblas_zgbmv(int o,int t,int m,int n,int kl,int ku,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_zgbmv(o,t,m,n,kl,ku,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_zhemv")))
void lind_cblas_zhemv(int o,int u,int n,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_zhemv(o,u,n,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_zhbmv")))
void lind_cblas_zhbmv(int o,int u,int n,int k,const void*al,const void*a,int lda,const void*x,int ix,const void*be,void*y,int iy){ cblas_zhbmv(o,u,n,k,al,a,lda,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_zhpmv")))
void lind_cblas_zhpmv(int o,int u,int n,const void*al,const void*ap,const void*x,int ix,const void*be,void*y,int iy){ cblas_zhpmv(o,u,n,al,ap,x,ix,be,y,iy); }
__attribute__((export_name("lind_cblas_ztrmv")))
void lind_cblas_ztrmv(int o,int u,int t,int d,int n,const void*a,int lda,void*x,int ix){ cblas_ztrmv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ztbmv")))
void lind_cblas_ztbmv(int o,int u,int t,int d,int n,int k,const void*a,int lda,void*x,int ix){ cblas_ztbmv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ztpmv")))
void lind_cblas_ztpmv(int o,int u,int t,int d,int n,const void*ap,void*x,int ix){ cblas_ztpmv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_ztrsv")))
void lind_cblas_ztrsv(int o,int u,int t,int d,int n,const void*a,int lda,void*x,int ix){ cblas_ztrsv(o,u,t,d,n,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ztbsv")))
void lind_cblas_ztbsv(int o,int u,int t,int d,int n,int k,const void*a,int lda,void*x,int ix){ cblas_ztbsv(o,u,t,d,n,k,a,lda,x,ix); }
__attribute__((export_name("lind_cblas_ztpsv")))
void lind_cblas_ztpsv(int o,int u,int t,int d,int n,const void*ap,void*x,int ix){ cblas_ztpsv(o,u,t,d,n,ap,x,ix); }
__attribute__((export_name("lind_cblas_zgeru")))
void lind_cblas_zgeru(int o,int m,int n,const void*al,const void*x,int ix,const void*y,int iy,void*a,int lda){ cblas_zgeru(o,m,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_zgerc")))
void lind_cblas_zgerc(int o,int m,int n,const void*al,const void*x,int ix,const void*y,int iy,void*a,int lda){ cblas_zgerc(o,m,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_zher")))
void lind_cblas_zher(int o,int u,int n,double al,const void*x,int ix,void*a,int lda){ cblas_zher(o,u,n,al,x,ix,a,lda); }
__attribute__((export_name("lind_cblas_zhpr")))
void lind_cblas_zhpr(int o,int u,int n,double al,const void*x,int ix,void*ap){ cblas_zhpr(o,u,n,al,x,ix,ap); }
__attribute__((export_name("lind_cblas_zher2")))
void lind_cblas_zher2(int o,int u,int n,const void*al,const void*x,int ix,const void*y,int iy,void*a,int lda){ cblas_zher2(o,u,n,al,x,ix,y,iy,a,lda); }
__attribute__((export_name("lind_cblas_zhpr2")))
void lind_cblas_zhpr2(int o,int u,int n,const void*al,const void*x,int ix,const void*y,int iy,void*ap){ cblas_zhpr2(o,u,n,al,x,ix,y,iy,ap); }


// --- complex level-3 --------------------------------------------------------------
__attribute__((export_name("lind_cblas_cgemm")))
void lind_cblas_cgemm(int o,int ta,int tb,int m,int n,int k,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_cgemm(o,ta,tb,m,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_csymm")))
void lind_cblas_csymm(int o,int s,int u,int m,int n,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_csymm(o,s,u,m,n,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_chemm")))
void lind_cblas_chemm(int o,int s,int u,int m,int n,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_chemm(o,s,u,m,n,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_csyrk")))
void lind_cblas_csyrk(int o,int u,int t,int n,int k,const void*al,const void*a,int lda,const void*be,void*c,int ldc){ cblas_csyrk(o,u,t,n,k,al,a,lda,be,c,ldc); }
__attribute__((export_name("lind_cblas_cherk")))
void lind_cblas_cherk(int o,int u,int t,int n,int k,float al,const void*a,int lda,float be,void*c,int ldc){ cblas_cherk(o,u,t,n,k,al,a,lda,be,c,ldc); }
__attribute__((export_name("lind_cblas_csyr2k")))
void lind_cblas_csyr2k(int o,int u,int t,int n,int k,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_csyr2k(o,u,t,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_cher2k")))
void lind_cblas_cher2k(int o,int u,int t,int n,int k,const void*al,const void*a,int lda,const void*b,int ldb,float be,void*c,int ldc){ cblas_cher2k(o,u,t,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_ctrmm")))
void lind_cblas_ctrmm(int o,int s,int u,int t,int d,int m,int n,const void*al,const void*a,int lda,void*b,int ldb){ cblas_ctrmm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }
__attribute__((export_name("lind_cblas_ctrsm")))
void lind_cblas_ctrsm(int o,int s,int u,int t,int d,int m,int n,const void*al,const void*a,int lda,void*b,int ldb){ cblas_ctrsm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }

__attribute__((export_name("lind_cblas_zgemm")))
void lind_cblas_zgemm(int o,int ta,int tb,int m,int n,int k,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_zgemm(o,ta,tb,m,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_zsymm")))
void lind_cblas_zsymm(int o,int s,int u,int m,int n,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_zsymm(o,s,u,m,n,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_zhemm")))
void lind_cblas_zhemm(int o,int s,int u,int m,int n,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_zhemm(o,s,u,m,n,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_zsyrk")))
void lind_cblas_zsyrk(int o,int u,int t,int n,int k,const void*al,const void*a,int lda,const void*be,void*c,int ldc){ cblas_zsyrk(o,u,t,n,k,al,a,lda,be,c,ldc); }
__attribute__((export_name("lind_cblas_zherk")))
void lind_cblas_zherk(int o,int u,int t,int n,int k,double al,const void*a,int lda,double be,void*c,int ldc){ cblas_zherk(o,u,t,n,k,al,a,lda,be,c,ldc); }
__attribute__((export_name("lind_cblas_zsyr2k")))
void lind_cblas_zsyr2k(int o,int u,int t,int n,int k,const void*al,const void*a,int lda,const void*b,int ldb,const void*be,void*c,int ldc){ cblas_zsyr2k(o,u,t,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_zher2k")))
void lind_cblas_zher2k(int o,int u,int t,int n,int k,const void*al,const void*a,int lda,const void*b,int ldb,double be,void*c,int ldc){ cblas_zher2k(o,u,t,n,k,al,a,lda,b,ldb,be,c,ldc); }
__attribute__((export_name("lind_cblas_ztrmm")))
void lind_cblas_ztrmm(int o,int s,int u,int t,int d,int m,int n,const void*al,const void*a,int lda,void*b,int ldb){ cblas_ztrmm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }
__attribute__((export_name("lind_cblas_ztrsm")))
void lind_cblas_ztrsm(int o,int s,int u,int t,int d,int m,int n,const void*al,const void*a,int lda,void*b,int ldb){ cblas_ztrsm(o,s,u,t,d,m,n,al,a,lda,b,ldb); }

