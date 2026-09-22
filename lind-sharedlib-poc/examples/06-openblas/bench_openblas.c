// Microbenchmark for OpenBLAS native vs sandboxed (lind) — see run_bench_sandboxed.sh.
//
// Separates the overheads that "native vs sandbox" conflates:
//   1. STARTUP    — first sandboxed call brings up the wasmtime runtime + preloads (one-time).
//   2. MARSHALLING— copy buffers host<->guest per call. Shows up in BLAS-1 (dot/axpy/scal):
//                   O(N) compute on O(N) data, so the copy never amortizes.
//   3. COMPUTE    — wasm portable-C kernels vs native tuned assembly. Shows up in BLAS-3
//                   (gemm): O(N^3) compute on O(N^2) data, so marshalling vanishes and the
//                   GFLOPS gap is pure kernel quality. BLAS-2 (gemv) sits in between.
//
// Coverage: gemm {s,d,c,z} (L3), gemv {s,d} (L2), axpy/dot/scal {d} (L1).
// Same source builds/links both ways; only the .so cblas_* resolves from differs.
// Times with CLOCK_MONOTONIC, warms up once per config, repeats and keeps the MIN.
//
// Human table by default; set BENCH_CSV=1 for machine-readable rows (impl from BENCH_IMPL).
// To add a routine: give it a bench_* fn following the pattern and call it in main().

#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <string.h>

// CBLAS enums / prototypes (avoid OpenBLAS's build-tree cblas.h).
enum { CblasRowMajor = 101, CblasColMajor = 102, CblasNoTrans = 111 };
extern void   cblas_sgemm(int,int,int,int,int,int,float, const float*,int,const float*,int,float, float*,int);
extern void   cblas_dgemm(int,int,int,int,int,int,double,const double*,int,const double*,int,double,double*,int);
extern void   cblas_cgemm(int,int,int,int,int,int,const void*,const void*,int,const void*,int,const void*,void*,int);
extern void   cblas_zgemm(int,int,int,int,int,int,const void*,const void*,int,const void*,int,const void*,void*,int);
extern void   cblas_sgemv(int,int,int,int,float, const float*,int,const float*,int,float, float*,int);
extern void   cblas_dgemv(int,int,int,int,double,const double*,int,const double*,int,double,double*,int);
extern void   cblas_daxpy(int,double,const double*,int,double*,int);
extern double cblas_ddot (int,const double*,int,const double*,int);
extern void   cblas_dscal(int,double,double*,int);

static int   g_csv = 0;
static const char *g_impl = "?";

static double now_s(void){ struct timespec ts; clock_gettime(CLOCK_MONOTONIC,&ts); return ts.tv_sec+ts.tv_nsec*1e-9; }
static double frand(void){ return (double)rand()/RAND_MAX - 0.5; }

// Uniform result line. flops<0 => report us/call (L1); else GFLOPS (L2/L3).
static void report(const char *routine, const char *prec, int n, double best_s, double flops){
    if (g_csv){
        double gf = flops > 0 ? flops/best_s/1e9 : 0.0;
        printf("%s,%s,%s,%d,%.6f,%.4f\n", g_impl, routine, prec, n, best_s*1e3, gf);
    } else if (flops > 0){
        printf("  %-6s %s N=%-5d %10.3f ms/call  %8.2f GFLOPS\n", routine, prec, n, best_s*1e3, flops/best_s/1e9);
    } else {
        printf("  %-6s %s N=%-7d %9.3f us/call\n", routine, prec, n, best_s*1e6);
    }
}

// ---- BLAS-3 gemm (compute-bound). Real s/d via macro, complex c/z separately. ----
#define DEF_GEMM_REAL(TAG, T, FN)                                                        \
static void bench_##TAG(int n, int reps){                                                \
    T *A=malloc((size_t)n*n*sizeof(T)),*B=malloc((size_t)n*n*sizeof(T)),*C=malloc((size_t)n*n*sizeof(T)); \
    for(int i=0;i<n*n;i++){A[i]=(T)frand();B[i]=(T)frand();C[i]=0;}                       \
    FN(CblasColMajor,CblasNoTrans,CblasNoTrans,n,n,n,(T)1,A,n,B,n,(T)0,C,n);              \
    double best=1e30;                                                                    \
    for(int r=0;r<reps;r++){double t=now_s();                                            \
        FN(CblasColMajor,CblasNoTrans,CblasNoTrans,n,n,n,(T)1,A,n,B,n,(T)0,C,n);          \
        double dt=now_s()-t; if(dt<best)best=dt;}                                         \
    report("gemm", #TAG, n, best, 2.0*n*n*n);                                            \
    free(A);free(B);free(C);                                                             \
}
DEF_GEMM_REAL(s, float,  cblas_sgemm)
DEF_GEMM_REAL(d, double, cblas_dgemm)

#define DEF_GEMM_CPLX(TAG, ESZ, FN)                                                      \
static void bench_##TAG(int n, int reps){                                                \
    size_t nb=(size_t)n*n*ESZ; char *A=malloc(nb),*B=malloc(nb),*C=malloc(nb);           \
    for(size_t i=0;i<nb;i++){A[i]=(char)(rand());B[i]=(char)(rand());} memset(C,0,nb);    \
    double al[2]={1,0}, be[2]={0,0};                                                     \
    FN(CblasColMajor,CblasNoTrans,CblasNoTrans,n,n,n,al,A,n,B,n,be,C,n);                 \
    double best=1e30;                                                                    \
    for(int r=0;r<reps;r++){double t=now_s();                                            \
        FN(CblasColMajor,CblasNoTrans,CblasNoTrans,n,n,n,al,A,n,B,n,be,C,n);             \
        double dt=now_s()-t; if(dt<best)best=dt;}                                         \
    report("gemm", #TAG, n, best, 8.0*n*n*n); /* complex gemm ~8 flops/MAC */            \
    free(A);free(B);free(C);                                                             \
}
DEF_GEMM_CPLX(c, 8,  cblas_cgemm)
DEF_GEMM_CPLX(z, 16, cblas_zgemm)

// ---- BLAS-2 gemv (intermediate: O(N^2) compute on O(N^2) data). ----
#define DEF_GEMV(TAG, T, FN)                                                             \
static void bench_gemv_##TAG(int n, int reps){                                           \
    T *A=malloc((size_t)n*n*sizeof(T)),*x=malloc(n*sizeof(T)),*y=malloc(n*sizeof(T));     \
    for(int i=0;i<n*n;i++)A[i]=(T)frand(); for(int i=0;i<n;i++){x[i]=(T)frand();y[i]=0;}  \
    FN(CblasColMajor,CblasNoTrans,n,n,(T)1,A,n,x,1,(T)0,y,1);                             \
    double best=1e30;                                                                    \
    for(int r=0;r<reps;r++){double t=now_s();                                            \
        FN(CblasColMajor,CblasNoTrans,n,n,(T)1,A,n,x,1,(T)0,y,1);                         \
        double dt=now_s()-t; if(dt<best)best=dt;}                                         \
    report("gemv", #TAG, n, best, 2.0*n*n);                                              \
    free(A);free(x);free(y);                                                             \
}
DEF_GEMV(s, float,  cblas_sgemv)
DEF_GEMV(d, double, cblas_dgemv)

// ---- BLAS-1 (marshalling-bound: report us/call). ----
static void bench_ddot(int n, int reps){
    double *x=malloc((size_t)n*sizeof(double)),*y=malloc((size_t)n*sizeof(double));
    for(int i=0;i<n;i++){x[i]=frand();y[i]=frand();}
    volatile double s=cblas_ddot(n,x,1,y,1);
    double best=1e30;
    for(int r=0;r<reps;r++){double t=now_s(); s=cblas_ddot(n,x,1,y,1); double dt=now_s()-t; if(dt<best)best=dt;}
    (void)s; report("ddot","d",n,best,-1); free(x);free(y);
}
static void bench_daxpy(int n, int reps){
    double *x=malloc((size_t)n*sizeof(double)),*y=malloc((size_t)n*sizeof(double));
    for(int i=0;i<n;i++){x[i]=frand();y[i]=frand();}
    cblas_daxpy(n,2.0,x,1,y,1);
    double best=1e30;
    for(int r=0;r<reps;r++){double t=now_s(); cblas_daxpy(n,2.0,x,1,y,1); double dt=now_s()-t; if(dt<best)best=dt;}
    report("daxpy","d",n,best,-1); free(x);free(y); // in/out y: copy-in AND copy-back
}
static void bench_dscal(int n, int reps){
    double *x=malloc((size_t)n*sizeof(double)); for(int i=0;i<n;i++)x[i]=frand();
    cblas_dscal(n,1.001,x,1);
    double best=1e30;
    for(int r=0;r<reps;r++){double t=now_s(); cblas_dscal(n,1.001,x,1); double dt=now_s()-t; if(dt<best)best=dt;}
    report("dscal","d",n,best,-1); free(x);
}

int main(void){
    srand(1);
    g_csv = getenv("BENCH_CSV") && atoi(getenv("BENCH_CSV"));
    if (getenv("BENCH_IMPL")) g_impl = getenv("BENCH_IMPL");
    if (g_csv) printf("impl,routine,prec,n,ms_per_call,gflops\n");

    // STARTUP: time the very first cblas call in this process.
    double xa[4]={1,2,3,4}, ya[4]={1,1,1,1};
    double t0=now_s(); volatile double s0=cblas_ddot(4,xa,1,ya,1); double startup=now_s()-t0; (void)s0;
    if (g_csv) printf("%s,startup,-,0,%.6f,0\n", g_impl, startup*1e3);
    else printf("startup (first call): %.3f ms\n\n", startup*1e3);

    int gn[]={64,128,256,512,1024}, gr[]={50,30,20,10,5};
    if(!g_csv) printf("BLAS-3 gemm (compute-bound):\n");
    for(unsigned i=0;i<sizeof(gn)/sizeof(*gn);i++){
        bench_s(gn[i],gr[i]); bench_d(gn[i],gr[i]); bench_c(gn[i],gr[i]); bench_z(gn[i],gr[i]);
    }

    int vn[]={256,512,1024,2048,4096}, vr[]={200,100,50,20,10};
    if(!g_csv) printf("\nBLAS-2 gemv (intermediate):\n");
    for(unsigned i=0;i<sizeof(vn)/sizeof(*vn);i++){ bench_gemv_s(vn[i],vr[i]); bench_gemv_d(vn[i],vr[i]); }

    int ln[]={100,1000,10000,100000,1000000};
    if(!g_csv) printf("\nBLAS-1 (marshalling-bound):\n");
    for(unsigned i=0;i<sizeof(ln)/sizeof(*ln);i++){ bench_ddot(ln[i],2000); bench_daxpy(ln[i],2000); bench_dscal(ln[i],2000); }

    return 0;
}
