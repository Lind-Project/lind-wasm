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

