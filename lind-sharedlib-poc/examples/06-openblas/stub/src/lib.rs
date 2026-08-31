// Hand-written (not gen_stubs.sh) — OpenBLAS array sizes are per-function BLAS
// semantics (element count from n/incx), which the simple manifest can't express, so
// the size logic lives here. Each cblas_* symbol is a native drop-in that marshals
// into the guest wrapper `lind_cblas_*` (see openblas_lind_shim.c) and runs the real
// BLAS inside the sandbox.
//
// Covers the CBLAS level-1 double set the reference test driver (ctest/c_dblas1.c)
// exercises: idamax, ddot, dnrm2, dasum, daxpy, dcopy, dswap, dscal, drot, drotg, drotm.

use std::sync::{Mutex, OnceLock};

use core::ffi::c_int;

use lind_boot::{Arg, CliOptions, OutLen, SandboxedLib, init_sandboxed_lib};

static LIB: OnceLock<Mutex<SandboxedLib>> = OnceLock::new();

/// Path to the precompiled OpenBLAS guest module (wasm library + shim, AOT'd to a
/// `.cwasm`). Overridable via `LIND_MODULE`.
fn module_path() -> String {
    std::env::var("LIND_MODULE").unwrap_or_else(|_| "libopenblas_lind.cwasm".to_string())
}

fn lib() -> &'static Mutex<SandboxedLib> {
    LIB.get_or_init(|| {
        let cli = CliOptions::for_sandboxed_lib(module_path());
        let sandboxed_lib = init_sandboxed_lib(cli)
            .unwrap_or_else(|e| panic!("lind sandboxed-lib init failed: {e:?}"));
        Mutex::new(sandboxed_lib)
    })
}

/// Call a void/integer-returning guest export.
fn call(name: &str, args: &mut [Arg]) -> i64 {
    lib()
        .lock()
        .unwrap()
        .call(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

/// Call an f64-returning guest export.
fn call_f64(name: &str, args: &mut [Arg]) -> f64 {
    lib()
        .lock()
        .unwrap()
        .call_f64(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

/// Number of elements a BLAS vector of logical length `n`, stride `inc` spans:
/// `1 + (n-1)*|inc|`. Zero for a non-positive length.
fn elems(n: c_int, inc: c_int) -> usize {
    if n <= 0 {
        0
    } else {
        1 + (n as usize - 1) * (inc.unsigned_abs() as usize)
    }
}

const F64: usize = 8; // bytes per double

/// Byte view of `n`-element/`inc`-stride input vector `p`.
///
/// # Safety
/// `p` must point to at least `elems(n, inc)` doubles.
unsafe fn vin<'a>(p: *const f64, n: c_int, inc: c_int) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(p as *const u8, elems(n, inc) * F64) }
}

/// Mutable byte view of `n`-element/`inc`-stride vector `p`.
///
/// # Safety
/// `p` must point to at least `elems(n, inc)` doubles.
unsafe fn vout<'a>(p: *mut f64, n: c_int, inc: c_int) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, elems(n, inc) * F64) }
}

/// Mutable byte view of a single double.
///
/// # Safety
/// `p` must point to a valid double.
unsafe fn scalar_out<'a>(p: *mut f64) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, F64) }
}

// --- CBLAS level-1 double ----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn cblas_idamax(n: c_int, x: *const f64, incx: c_int) -> usize {
    let xb = unsafe { vin(x, n, incx) };
    call("lind_cblas_idamax", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)]) as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_ddot(
    n: c_int,
    x: *const f64,
    incx: c_int,
    y: *const f64,
    incy: c_int,
) -> f64 {
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vin(y, n, incy) };
    call_f64(
        "lind_cblas_ddot",
        &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy)],
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dnrm2(n: c_int, x: *const f64, incx: c_int) -> f64 {
    let xb = unsafe { vin(x, n, incx) };
    call_f64("lind_cblas_dnrm2", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dasum(n: c_int, x: *const f64, incx: c_int) -> f64 {
    let xb = unsafe { vin(x, n, incx) };
    call_f64("lind_cblas_dasum", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_daxpy(
    n: c_int,
    alpha: f64,
    x: *const f64,
    incx: c_int,
    y: *mut f64,
    incy: c_int,
) {
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vout(y, n, incy) }; // y := alpha*x + y  (read + write)
    call(
        "lind_cblas_daxpy",
        &mut [
            Arg::I32(n),
            Arg::F64(alpha),
            Arg::Buf(xb),
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dcopy(n: c_int, x: *const f64, incx: c_int, y: *mut f64, incy: c_int) {
    let xb = unsafe { vin(x, n, incx) };
    // y := x. Although dcopy only *writes* the strided elements, a strided buffer
    // (|incy| > 1) has GAPS the routine never touches, and callers expect those
    // untouched bytes preserved. `Out` would leave the gaps as guest garbage and copy
    // them back over the caller's originals, so seed the buffer first: use InOut.
    let yb = unsafe { vout(y, n, incy) };
    call(
        "lind_cblas_dcopy",
        &mut [
            Arg::I32(n),
            Arg::Buf(xb),
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dswap(n: c_int, x: *mut f64, incx: c_int, y: *mut f64, incy: c_int) {
    let xb = unsafe { vout(x, n, incx) }; // x <-> y  (both in/out)
    let yb = unsafe { vout(y, n, incy) };
    call(
        "lind_cblas_dswap",
        &mut [
            Arg::I32(n),
            Arg::InOut { dst: xb, len: OutLen::Cap },
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dscal(n: c_int, alpha: f64, x: *mut f64, incx: c_int) {
    let xb = unsafe { vout(x, n, incx) }; // x := alpha*x  (in/out)
    call(
        "lind_cblas_dscal",
        &mut [Arg::I32(n), Arg::F64(alpha), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_drot(
    n: c_int,
    x: *mut f64,
    incx: c_int,
    y: *mut f64,
    incy: c_int,
    c: f64,
    s: f64,
) {
    let xb = unsafe { vout(x, n, incx) }; // plane rotation applied to x,y in place
    let yb = unsafe { vout(y, n, incy) };
    call(
        "lind_cblas_drot",
        &mut [
            Arg::I32(n),
            Arg::InOut { dst: xb, len: OutLen::Cap },
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
            Arg::F64(c),
            Arg::F64(s),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_drotg(a: *mut f64, b: *mut f64, c: *mut f64, s: *mut f64) {
    // a,b are in/out (a<-r, b<-z); c,s are outputs. Each is a single double.
    let ab = unsafe { scalar_out(a) };
    let bb = unsafe { scalar_out(b) };
    let cb = unsafe { scalar_out(c) };
    let sb = unsafe { scalar_out(s) };
    call(
        "lind_cblas_drotg",
        &mut [
            Arg::InOut { dst: ab, len: OutLen::Cap },
            Arg::InOut { dst: bb, len: OutLen::Cap },
            Arg::Out { dst: cb, len: OutLen::Cap },
            Arg::Out { dst: sb, len: OutLen::Cap },
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_drotm(
    n: c_int,
    x: *mut f64,
    incx: c_int,
    y: *mut f64,
    incy: c_int,
    param: *const f64,
) {
    let xb = unsafe { vout(x, n, incx) }; // modified rotation applied to x,y in place
    let yb = unsafe { vout(y, n, incy) };
    let pb = unsafe { core::slice::from_raw_parts(param as *const u8, 5 * F64) }; // flag + 2x2 H
    call(
        "lind_cblas_drotm",
        &mut [
            Arg::I32(n),
            Arg::InOut { dst: xb, len: OutLen::Cap },
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
            Arg::Buf(pb),
        ],
    );
}

// --- Fortran-ABI forwarders --------------------------------------------------------
// The reference CBLAS level-1 driver (ctest/c_dblat1c.c) calls the Fortran symbols
// `drot_`/`drotm_` directly for the rotation routines (rather than the cblas_* wrappers).
// In a native build those resolve from libopenblas.a; here we provide them so the driver
// links against our .so, forwarding to the sandboxed cblas_* (so they're tested too).
// Fortran ABI = every argument by pointer.

#[unsafe(no_mangle)]
pub extern "C" fn drot_(
    n: *const c_int,
    x: *mut f64,
    incx: *const c_int,
    y: *mut f64,
    incy: *const c_int,
    c: *const f64,
    s: *const f64,
) {
    unsafe { cblas_drot(*n, x, *incx, y, *incy, *c, *s) }
}

#[unsafe(no_mangle)]
pub extern "C" fn drotm_(
    n: *const c_int,
    x: *mut f64,
    incx: *const c_int,
    y: *mut f64,
    incy: *const c_int,
    param: *const f64,
) {
    unsafe { cblas_drotm(*n, x, *incx, y, *incy, param) }
}

// ===================================================================================
// CBLAS level-1 single (f32). Mirrors the double set: 4-byte elements, `Arg::F32`
// scalars, `call_f32` returns. As with dcopy, scopy's strided output uses InOut.
// ===================================================================================

const F32: usize = 4; // bytes per float

/// Call an f32-returning guest export.
fn call_f32(name: &str, args: &mut [Arg]) -> f32 {
    lib()
        .lock()
        .unwrap()
        .call_f32(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

/// # Safety
/// `p` must point to at least `elems(n, inc)` floats.
unsafe fn sin<'a>(p: *const f32, n: c_int, inc: c_int) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(p as *const u8, elems(n, inc) * F32) }
}

/// # Safety
/// `p` must point to at least `elems(n, inc)` floats.
unsafe fn sout<'a>(p: *mut f32, n: c_int, inc: c_int) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, elems(n, inc) * F32) }
}

/// # Safety
/// `p` must point to a valid float.
unsafe fn sscalar_out<'a>(p: *mut f32) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, F32) }
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_isamax(n: c_int, x: *const f32, incx: c_int) -> usize {
    let xb = unsafe { sin(x, n, incx) };
    call("lind_cblas_isamax", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)]) as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_sdot(
    n: c_int,
    x: *const f32,
    incx: c_int,
    y: *const f32,
    incy: c_int,
) -> f32 {
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sin(y, n, incy) };
    call_f32(
        "lind_cblas_sdot",
        &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy)],
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_snrm2(n: c_int, x: *const f32, incx: c_int) -> f32 {
    let xb = unsafe { sin(x, n, incx) };
    call_f32("lind_cblas_snrm2", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_sasum(n: c_int, x: *const f32, incx: c_int) -> f32 {
    let xb = unsafe { sin(x, n, incx) };
    call_f32("lind_cblas_sasum", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_saxpy(
    n: c_int,
    alpha: f32,
    x: *const f32,
    incx: c_int,
    y: *mut f32,
    incy: c_int,
) {
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    call(
        "lind_cblas_saxpy",
        &mut [
            Arg::I32(n),
            Arg::F32(alpha),
            Arg::Buf(xb),
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_scopy(n: c_int, x: *const f32, incx: c_int, y: *mut f32, incy: c_int) {
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) }; // strided output -> InOut (preserve gaps)
    call(
        "lind_cblas_scopy",
        &mut [
            Arg::I32(n),
            Arg::Buf(xb),
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_sswap(n: c_int, x: *mut f32, incx: c_int, y: *mut f32, incy: c_int) {
    let xb = unsafe { sout(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    call(
        "lind_cblas_sswap",
        &mut [
            Arg::I32(n),
            Arg::InOut { dst: xb, len: OutLen::Cap },
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_sscal(n: c_int, alpha: f32, x: *mut f32, incx: c_int) {
    let xb = unsafe { sout(x, n, incx) };
    call(
        "lind_cblas_sscal",
        &mut [Arg::I32(n), Arg::F32(alpha), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_srot(
    n: c_int,
    x: *mut f32,
    incx: c_int,
    y: *mut f32,
    incy: c_int,
    c: f32,
    s: f32,
) {
    let xb = unsafe { sout(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    call(
        "lind_cblas_srot",
        &mut [
            Arg::I32(n),
            Arg::InOut { dst: xb, len: OutLen::Cap },
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
            Arg::F32(c),
            Arg::F32(s),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_srotg(a: *mut f32, b: *mut f32, c: *mut f32, s: *mut f32) {
    let ab = unsafe { sscalar_out(a) };
    let bb = unsafe { sscalar_out(b) };
    let cb = unsafe { sscalar_out(c) };
    let sb = unsafe { sscalar_out(s) };
    call(
        "lind_cblas_srotg",
        &mut [
            Arg::InOut { dst: ab, len: OutLen::Cap },
            Arg::InOut { dst: bb, len: OutLen::Cap },
            Arg::Out { dst: cb, len: OutLen::Cap },
            Arg::Out { dst: sb, len: OutLen::Cap },
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_srotm(
    n: c_int,
    x: *mut f32,
    incx: c_int,
    y: *mut f32,
    incy: c_int,
    param: *const f32,
) {
    let xb = unsafe { sout(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    let pb = unsafe { core::slice::from_raw_parts(param as *const u8, 5 * F32) };
    call(
        "lind_cblas_srotm",
        &mut [
            Arg::I32(n),
            Arg::InOut { dst: xb, len: OutLen::Cap },
            Arg::I32(incx),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
            Arg::Buf(pb),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn srot_(
    n: *const c_int,
    x: *mut f32,
    incx: *const c_int,
    y: *mut f32,
    incy: *const c_int,
    c: *const f32,
    s: *const f32,
) {
    unsafe { cblas_srot(*n, x, *incx, y, *incy, *c, *s) }
}

#[unsafe(no_mangle)]
pub extern "C" fn srotm_(
    n: *const c_int,
    x: *mut f32,
    incx: *const c_int,
    y: *mut f32,
    incy: *const c_int,
    param: *const f32,
) {
    unsafe { cblas_srotm(*n, x, *incx, y, *incy, param) }
}

// ===================================================================================
// CBLAS level-2 — general matrix-vector (gemv). Introduces the leading dimension:
// a matrix is NOT contiguous, it's stored with `lda` stride between columns, so the
// buffer spans `lda * cols` elements with gap rows. `A` is read-only here (Buf); `y`
// is read+written (InOut). Enum args (order/trans) are plain ints.
// ===================================================================================

// CBLAS enum values (from cblas.h).
const CBLAS_ROW_MAJOR: c_int = 101;
const CBLAS_COL_MAJOR: c_int = 102;
const CBLAS_NO_TRANS: c_int = 111;

/// Elements spanned by an `m`×`n` general matrix stored with leading dimension `lda`:
/// column-major packs `n` columns of `lda` (`lda ≥ m`); row-major packs `m` rows of
/// `lda` (`lda ≥ n`).
fn gemat_elems(order: c_int, m: c_int, n: c_int, lda: c_int) -> usize {
    let units = if order == CBLAS_ROW_MAJOR { m } else { n };
    (lda.max(0) as usize) * (units.max(0) as usize)
}

/// (len x, len y) for `y := op(A)·x`: NoTrans → A is m×n so x∈ℝⁿ, y∈ℝᵐ; else swapped.
fn gemv_vec_lens(trans: c_int, m: c_int, n: c_int) -> (c_int, c_int) {
    if trans == CBLAS_NO_TRANS { (n, m) } else { (m, n) }
}

/// Elements of an n×n square matrix (symmetric/triangular) with leading dimension lda.
/// Order/uplo don't change the storage span for a square matrix.
fn sqmat_elems(n: c_int, lda: c_int) -> usize {
    (lda.max(0) as usize) * (n.max(0) as usize)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dgemv(
    order: c_int,
    trans: c_int,
    m: c_int,
    n: c_int,
    alpha: f64,
    a: *const f64,
    lda: c_int,
    x: *const f64,
    incx: c_int,
    beta: f64,
    y: *mut f64,
    incy: c_int,
) {
    let ab = unsafe {
        core::slice::from_raw_parts(a as *const u8, gemat_elems(order, m, n, lda) * F64)
    };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { vin(x, lenx, incx) };
    let yb = unsafe { vout(y, leny, incy) }; // y read (if beta≠0) + written -> InOut
    call(
        "lind_cblas_dgemv",
        &mut [
            Arg::I32(order),
            Arg::I32(trans),
            Arg::I32(m),
            Arg::I32(n),
            Arg::F64(alpha),
            Arg::Buf(ab),
            Arg::I32(lda),
            Arg::Buf(xb),
            Arg::I32(incx),
            Arg::F64(beta),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_sgemv(
    order: c_int,
    trans: c_int,
    m: c_int,
    n: c_int,
    alpha: f32,
    a: *const f32,
    lda: c_int,
    x: *const f32,
    incx: c_int,
    beta: f32,
    y: *mut f32,
    incy: c_int,
) {
    let ab = unsafe {
        core::slice::from_raw_parts(a as *const u8, gemat_elems(order, m, n, lda) * F32)
    };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { sin(x, lenx, incx) };
    let yb = unsafe { sout(y, leny, incy) };
    call(
        "lind_cblas_sgemv",
        &mut [
            Arg::I32(order),
            Arg::I32(trans),
            Arg::I32(m),
            Arg::I32(n),
            Arg::F32(alpha),
            Arg::Buf(ab),
            Arg::I32(lda),
            Arg::Buf(xb),
            Arg::I32(incx),
            Arg::F32(beta),
            Arg::InOut { dst: yb, len: OutLen::Cap },
            Arg::I32(incy),
        ],
    );
}


// --- level-2 general/square group (symv, trmv, trsv, ger, syr, syr2) ---------------
// All enum args (order/uplo/trans/diag) pass through as I32. Square matrices (symv,
// trmv, trsv, syr, syr2) span `lda*n`; ger's general A uses gemat_elems. A is read-only
// for symv/trmv/trsv (Buf) and in/out for ger/syr/syr2 (InOut, rank-k updates). trmv/
// trsv transform x in place (InOut).

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsymv(order: c_int, uplo: c_int, n: c_int, alpha: f64, a: *const f64, lda: c_int, x: *const f64, incx: c_int, beta: f64, y: *mut f64, incy: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, sqmat_elems(n, lda) * F64) };
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vout(y, n, incy) };
    call("lind_cblas_dsymv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::F64(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dtrmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const f64, lda: c_int, x: *mut f64, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, sqmat_elems(n, lda) * F64) };
    let xb = unsafe { vout(x, n, incx) }; // x := op(A)·x  (in place)
    call("lind_cblas_dtrmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dtrsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const f64, lda: c_int, x: *mut f64, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, sqmat_elems(n, lda) *F64) };
    let xb = unsafe { vout(x, n, incx) }; // solve op(A)·x = b  (x overwritten)
    call("lind_cblas_dtrsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dger(order: c_int, m: c_int, n: c_int, alpha: f64, x: *const f64, incx: c_int, y: *const f64, incy: c_int, a: *mut f64, lda: c_int) {
    let xb = unsafe { vin(x, m, incx) };
    let yb = unsafe { vin(y, n, incy) };
    let ab = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, gemat_elems(order, m, n, lda) * F64) };
    call("lind_cblas_dger", &mut [Arg::I32(order), Arg::I32(m), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsyr(order: c_int, uplo: c_int, n: c_int, alpha: f64, x: *const f64, incx: c_int, a: *mut f64, lda: c_int) {
    let xb = unsafe { vin(x, n, incx) };
    let ab = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, sqmat_elems(n, lda) * F64) };
    call("lind_cblas_dsyr", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsyr2(order: c_int, uplo: c_int, n: c_int, alpha: f64, x: *const f64, incx: c_int, y: *const f64, incy: c_int, a: *mut f64, lda: c_int) {
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vin(y, n, incy) };
    let ab = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, sqmat_elems(n, lda) * F64) };
    call("lind_cblas_dsyr2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}

// single-precision twins

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssymv(order: c_int, uplo: c_int, n: c_int, alpha: f32, a: *const f32, lda: c_int, x: *const f32, incx: c_int, beta: f32, y: *mut f32, incy: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, sqmat_elems(n, lda) * F32) };
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    call("lind_cblas_ssymv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::F32(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_strmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const f32, lda: c_int, x: *mut f32, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, sqmat_elems(n, lda) * F32) };
    let xb = unsafe { sout(x, n, incx) };
    call("lind_cblas_strmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_strsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const f32, lda: c_int, x: *mut f32, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, sqmat_elems(n, lda) * F32) };
    let xb = unsafe { sout(x, n, incx) };
    call("lind_cblas_strsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_sger(order: c_int, m: c_int, n: c_int, alpha: f32, x: *const f32, incx: c_int, y: *const f32, incy: c_int, a: *mut f32, lda: c_int) {
    let xb = unsafe { sin(x, m, incx) };
    let yb = unsafe { sin(y, n, incy) };
    let ab = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, gemat_elems(order, m, n, lda) * F32) };
    call("lind_cblas_sger", &mut [Arg::I32(order), Arg::I32(m), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssyr(order: c_int, uplo: c_int, n: c_int, alpha: f32, x: *const f32, incx: c_int, a: *mut f32, lda: c_int) {
    let xb = unsafe { sin(x, n, incx) };
    let ab = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, sqmat_elems(n, lda) * F32) };
    call("lind_cblas_ssyr", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssyr2(order: c_int, uplo: c_int, n: c_int, alpha: f32, x: *const f32, incx: c_int, y: *const f32, incy: c_int, a: *mut f32, lda: c_int) {
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sin(y, n, incy) };
    let ab = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, sqmat_elems(n, lda) * F32) };
    call("lind_cblas_ssyr2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst:ab, len: OutLen::Cap }, Arg::I32(lda)]);
}


// --- level-2 banded + packed groups -----------------------------------------------
// Banded A spans (n+band)*lda (matches the reference driver's allocation; band = kl for
// gbmv, k for sbmv/tbmv/tbsv). Packed AP is a contiguous triangle of n(n+1)/2 (no lda,
// no gaps). A/AP read-only except spr/spr2 (InOut). tbmv/tbsv/tpmv/tpsv transform x.

fn bandmat_elems(n: c_int, band: c_int, lda: c_int) -> usize {
    ((n.max(0) + band.max(0)) as usize) * (lda.max(0) as usize)
}
fn packed_elems(n: c_int) -> usize {
    let n = n.max(0) as usize;
    n * (n + 1) / 2
}

// banded (double)
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dgbmv(order: c_int, trans: c_int, m: c_int, n: c_int, kl: c_int, ku: c_int, alpha: f64, a: *const f64, lda: c_int, x: *const f64, incx: c_int, beta: f64, y: *mut f64, incy: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, kl, lda) * F64) };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { vin(x, lenx, incx) };
    let yb = unsafe { vout(y, leny, incy) };
    call("lind_cblas_dgbmv", &mut [Arg::I32(order), Arg::I32(trans), Arg::I32(m), Arg::I32(n), Arg::I32(kl), Arg::I32(ku), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::F64(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsbmv(order: c_int, uplo: c_int, n: c_int, k: c_int, alpha: f64, a: *const f64, lda: c_int, x: *const f64, incx: c_int, beta: f64, y: *mut f64, incy: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, k, lda) * F64) };
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vout(y, n, incy) };
    call("lind_cblas_dsbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::I32(k), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::F64(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dtbmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const f64, lda: c_int, x: *mut f64, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, k, lda) * F64) };
    let xb = unsafe { vout(x, n, incx) };
    call("lind_cblas_dtbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dtbsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const f64, lda: c_int, x: *mut f64, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, k, lda) * F64) };
    let xb = unsafe { vout(x, n, incx) };
    call("lind_cblas_dtbsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

// packed (double)
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dspmv(order: c_int, uplo: c_int, n: c_int, alpha: f64, ap: *const f64, x: *const f64, incx: c_int, beta: f64, y: *mut f64, incy: c_int) {
    let apb = unsafe { core::slice::from_raw_parts(ap as *const u8, packed_elems(n) * F64) };
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vout(y, n, incy) };
    call("lind_cblas_dspmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(apb), Arg::Buf(xb), Arg::I32(incx), Arg::F64(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_dspr(order: c_int, uplo: c_int, n: c_int, alpha: f64, x: *const f64, incx: c_int, ap: *mut f64) {
    let xb = unsafe { vin(x, n, incx) };
    let apb = unsafe { core::slice::from_raw_parts_mut(ap as *mut u8, packed_elems(n) * F64) };
    call("lind_cblas_dspr", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_dtpmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const f64, x: *mut f64, incx: c_int) {
    let apb = unsafe { core::slice::from_raw_parts(ap as *const u8, packed_elems(n) * F64) };
    let xb = unsafe { vout(x, n, incx) };
    call("lind_cblas_dtpmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_dtpsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const f64, x: *mut f64, incx: c_int) {
    let apb = unsafe { core::slice::from_raw_parts(ap as *const u8, packed_elems(n) * F64) };
    let xb = unsafe { vout(x, n, incx) };
    call("lind_cblas_dtpsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dspr2(order: c_int, uplo: c_int, n: c_int, alpha: f64, x: *const f64, incx: c_int, y: *const f64, incy: c_int, ap: *mut f64) {
    let xb = unsafe { vin(x, n, incx) };
    let yb = unsafe { vin(y, n, incy) };
    let apb = unsafe { core::slice::from_raw_parts_mut(ap as *mut u8, packed_elems(n) * F64) };
    call("lind_cblas_dspr2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}

// banded (single)
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_sgbmv(order: c_int, trans: c_int, m: c_int, n: c_int, kl: c_int, ku: c_int, alpha: f32, a: *const f32, lda: c_int, x: *const f32, incx: c_int, beta: f32, y: *mut f32, incy: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, kl, lda) * F32) };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { sin(x, lenx, incx) };
    let yb = unsafe { sout(y, leny, incy) };
    call("lind_cblas_sgbmv", &mut [Arg::I32(order), Arg::I32(trans), Arg::I32(m), Arg::I32(n), Arg::I32(kl), Arg::I32(ku), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::F32(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssbmv(order: c_int, uplo: c_int, n: c_int, k: c_int, alpha: f32, a: *const f32, lda: c_int, x: *const f32, incx: c_int, beta: f32, y: *mut f32, incy: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, k, lda) * F32) };
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    call("lind_cblas_ssbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::I32(k), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::F32(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_stbmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const f32, lda: c_int, x: *mut f32, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, k, lda) * F32) };
    let xb = unsafe { sout(x, n, incx) };
    call("lind_cblas_stbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_stbsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const f32, lda: c_int, x: *mut f32, incx: c_int) {
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, bandmat_elems(n, k, lda) * F32) };
    let xb = unsafe { sout(x, n, incx) };
    call("lind_cblas_stbsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

// packed (single)
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_sspmv(order: c_int, uplo: c_int, n: c_int, alpha: f32, ap: *const f32, x: *const f32, incx: c_int, beta: f32, y: *mut f32, incy: c_int) {
    let apb = unsafe { core::slice::from_raw_parts(ap as *const u8, packed_elems(n) * F32) };
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sout(y, n, incy) };
    call("lind_cblas_sspmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(apb), Arg::Buf(xb), Arg::I32(incx), Arg::F32(beta), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_sspr(order: c_int, uplo: c_int, n: c_int, alpha: f32, x: *const f32, incx: c_int, ap: *mut f32) {
    let xb = unsafe { sin(x, n, incx) };
    let apb = unsafe { core::slice::from_raw_parts_mut(ap as *mut u8, packed_elems(n) * F32) };
    call("lind_cblas_sspr", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_stpmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const f32, x: *mut f32, incx: c_int) {
    let apb = unsafe { core::slice::from_raw_parts(ap as *const u8, packed_elems(n) * F32) };
    let xb = unsafe { sout(x, n, incx) };
    call("lind_cblas_stpmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_stpsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const f32, x: *mut f32, incx: c_int) {
    let apb = unsafe { core::slice::from_raw_parts(ap as *const u8, packed_elems(n) * F32) };
    let xb = unsafe { sout(x, n, incx) };
    call("lind_cblas_stpsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_sspr2(order: c_int, uplo: c_int, n: c_int, alpha: f32, x: *const f32, incx: c_int, y: *const f32, incy: c_int, ap: *mut f32) {
    let xb = unsafe { sin(x, n, incx) };
    let yb = unsafe { sin(y, n, incy) };
    let apb = unsafe { core::slice::from_raw_parts_mut(ap as *mut u8, packed_elems(n) * F32) };
    call("lind_cblas_sspr2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}


// ===================================================================================
// CBLAS level-3 (gemm, symm, syrk, syr2k, trmm, trsm). Every operand is a full 2D
// matrix sized gemat_elems(order, R, C, ld); the stored shape (R,C) is flipped by
// `trans` (trans_dims) or square by `side` (side_dim). A/B are read-only (Buf); C is
// in/out for gemm/symm/syrk/syr2k; B is in/out for trmm/trsm.
// ===================================================================================

const CBLAS_LEFT: c_int = 141;

/// Stored (rows, cols) of op(X): a×b when NoTrans, else b×a.
fn trans_dims(trans: c_int, a: c_int, b: c_int) -> (c_int, c_int) {
    if trans == CBLAS_NO_TRANS { (a, b) } else { (b, a) }
}
/// Order of the square matrix A: m when side is Left, else n.
fn side_dim(side: c_int, m: c_int, n: c_int) -> c_int {
    if side == CBLAS_LEFT { m } else { n }
}

// double

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dgemm(order: c_int, transa: c_int, transb: c_int, m: c_int, n: c_int, k: c_int, alpha: f64, a: *const f64, lda: c_int, b: *const f64, ldb: c_int, beta: f64, c: *mut f64, ldc: c_int) {
    let (ar, ac) = trans_dims(transa, m, k);
    let (br, bc) = trans_dims(transb, k, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ar, ac, lda) * F64) };
    let bb = unsafe { core::slice::from_raw_parts(b as *const u8, gemat_elems(order, br, bc, ldb) * F64) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, m, n, ldc) * F64) };
    call("lind_cblas_dgemm", &mut [Arg::I32(order), Arg::I32(transa), Arg::I32(transb), Arg::I32(m), Arg::I32(n), Arg::I32(k), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F64(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsymm(order: c_int, side: c_int, uplo: c_int, m: c_int, n: c_int, alpha: f64, a: *const f64, lda: c_int, b: *const f64, ldb: c_int, beta: f64, c: *mut f64, ldc: c_int) {
    let ad = side_dim(side, m, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ad, ad, lda) * F64) };
    let bb = unsafe { core::slice::from_raw_parts(b as *const u8, gemat_elems(order, m, n, ldb) * F64) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, m, n, ldc) * F64) };
    call("lind_cblas_dsymm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(m), Arg::I32(n), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F64(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsyrk(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: f64, a: *const f64, lda: c_int, beta: f64, c: *mut f64, ldc: c_int) {
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ar, ac, lda) * F64) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, n, n, ldc) * F64) };
    call("lind_cblas_dsyrk", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::F64(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dsyr2k(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: f64, a: *const f64, lda: c_int, b: *const f64, ldb: c_int, beta: f64, c: *mut f64, ldc: c_int) {
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ar, ac, lda) * F64) };
    let bb = unsafe { core::slice::from_raw_parts(b as *const u8, gemat_elems(order, ar, ac, ldb) * F64) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, n, n, ldc) * F64) };
    call("lind_cblas_dsyr2k", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F64(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dtrmm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: f64, a: *const f64, lda: c_int, b: *mut f64, ldb: c_int) {
    let ad = side_dim(side, m, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ad, ad, lda) * F64) };
    let bb = unsafe { core::slice::from_raw_parts_mut(b as *mut u8, gemat_elems(order, m, n, ldb) * F64) };
    call("lind_cblas_dtrmm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_dtrsm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: f64, a: *const f64, lda: c_int, b: *mut f64, ldb: c_int) {
    let ad = side_dim(side, m, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ad, ad, lda) * F64) };
    let bb = unsafe { core::slice::from_raw_parts_mut(b as *mut u8, gemat_elems(order, m, n, ldb) * F64) };
    call("lind_cblas_dtrsm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}

// single

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_sgemm(order: c_int, transa: c_int, transb: c_int, m: c_int, n: c_int, k: c_int, alpha: f32, a: *const f32, lda: c_int, b: *const f32, ldb: c_int, beta: f32, c: *mut f32, ldc: c_int) {
    let (ar, ac) = trans_dims(transa, m, k);
    let (br, bc) = trans_dims(transb, k, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ar, ac, lda) * F32) };
    let bb = unsafe { core::slice::from_raw_parts(b as *const u8, gemat_elems(order, br, bc, ldb) * F32) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, m, n, ldc) * F32) };
    call("lind_cblas_sgemm", &mut [Arg::I32(order), Arg::I32(transa), Arg::I32(transb), Arg::I32(m), Arg::I32(n), Arg::I32(k), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F32(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssymm(order: c_int, side: c_int, uplo: c_int, m: c_int, n: c_int, alpha: f32, a: *const f32, lda: c_int, b: *const f32, ldb: c_int, beta: f32, c: *mut f32, ldc: c_int) {
    let ad = side_dim(side, m, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ad, ad, lda) * F32) };
    let bb = unsafe { core::slice::from_raw_parts(b as *const u8, gemat_elems(order, m, n, ldb) * F32) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, m, n, ldc) * F32) };
    call("lind_cblas_ssymm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(m), Arg::I32(n), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F32(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssyrk(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: f32, a: *const f32, lda: c_int, beta: f32, c: *mut f32, ldc: c_int) {
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ar, ac, lda) * F32) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, n, n, ldc) * F32) };
    call("lind_cblas_ssyrk", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::F32(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ssyr2k(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: f32, a: *const f32, lda: c_int, b: *const f32, ldb: c_int, beta: f32, c: *mut f32, ldc: c_int) {
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ar, ac, lda) * F32) };
    let bb = unsafe { core::slice::from_raw_parts(b as *const u8, gemat_elems(order, ar, ac, ldb) * F32) };
    let cb = unsafe { core::slice::from_raw_parts_mut(c as *mut u8, gemat_elems(order, n, n, ldc) * F32) };
    call("lind_cblas_ssyr2k", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F32(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_strmm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: f32, a: *const f32, lda: c_int, b: *mut f32, ldb: c_int) {
    let ad = side_dim(side, m, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ad, ad, lda) * F32) };
    let bb = unsafe { core::slice::from_raw_parts_mut(b as *mut u8, gemat_elems(order, m, n, ldb) * F32) };
    call("lind_cblas_strmm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_strsm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: f32, a: *const f32, lda: c_int, b: *mut f32, ldb: c_int) {
    let ad = side_dim(side, m, n);
    let ab = unsafe { core::slice::from_raw_parts(a as *const u8, gemat_elems(order, ad, ad, lda) * F32) };
    let bb = unsafe { core::slice::from_raw_parts_mut(b as *mut u8, gemat_elems(order, m, n, ldb) * F32) };
    call("lind_cblas_strsm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}


// ===================================================================================
// CBLAS complex level-1 (c = single-complex, 8-byte elems; z = double-complex, 16-byte).
// Three patterns differ from the real level-1 sets:
//   * A complex scalar (alpha) crosses by POINTER, not by value: cblas_?axpy/?scal take
//     `const void *alpha` -> marshal it as Arg::Buf of one complex element.
//   * The complex dot product returns through an OUT pointer (cblas_?dotu_sub /
//     ?dotc_sub, `void *dot`) -> Arg::Out of one element (fully written, contiguous).
//   * The norm/asum reductions return a REAL: scnrm2/scasum -> f32, dznrm2/dzasum -> f64.
//   * ?sscal/?dscal scale a complex vector by a REAL scalar passed by value (F32/F64).
// Element COUNTS reuse elems()/the real size helpers; only the byte width changes.
// ===================================================================================

use core::ffi::c_void;

const C64: usize = 8; // bytes per single-complex value (2 x f32)
const C128: usize = 16; // bytes per double-complex value (2 x f64)

/// Byte view of an `n`-element/`inc`-stride complex input vector (`elem` bytes/element).
///
/// # Safety
/// `p` must point to at least `elems(n, inc)` complex values of `elem` bytes.
unsafe fn cxin<'a>(p: *const c_void, n: c_int, inc: c_int, elem: usize) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(p as *const u8, elems(n, inc) * elem) }
}

/// Mutable byte view of an `n`-element/`inc`-stride complex vector.
///
/// # Safety
/// As `cxin`.
unsafe fn cxout<'a>(p: *mut c_void, n: c_int, inc: c_int, elem: usize) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, elems(n, inc) * elem) }
}

/// Byte view of one complex value — a by-pointer alpha/beta scalar (`elem` bytes).
///
/// # Safety
/// `p` must point to at least `elem` readable bytes.
unsafe fn cxscalar<'a>(p: *const c_void, elem: usize) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(p as *const u8, elem) }
}

/// Mutable byte view of one complex value — a by-pointer output (e.g. a dot result).
///
/// # Safety
/// `p` must point to at least `elem` writable bytes.
unsafe fn cxscalar_out<'a>(p: *mut c_void, elem: usize) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, elem) }
}

/// Byte view of a complex matrix of `elems` complex values (`elem` bytes each).
///
/// # Safety
/// `p` must point to at least `elems` complex values of `elem` bytes.
unsafe fn cmat_in<'a>(p: *const c_void, elems: usize, elem: usize) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(p as *const u8, elems * elem) }
}

/// Mutable byte view of a complex matrix of `elems` complex values.
///
/// # Safety
/// As `cmat_in`.
unsafe fn cmat_out<'a>(p: *mut c_void, elems: usize, elem: usize) -> &'a mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(p as *mut u8, elems * elem) }
}


// --- single-complex (c) ------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn cblas_icamax(n: c_int, x: *const c_void, incx: c_int) -> usize {
    let xb = unsafe { cxin(x, n, incx, C64) };
    call("lind_cblas_icamax", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)]) as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_scnrm2(n: c_int, x: *const c_void, incx: c_int) -> f32 {
    let xb = unsafe { cxin(x, n, incx, C64) };
    call_f32("lind_cblas_scnrm2", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_scasum(n: c_int, x: *const c_void, incx: c_int) -> f32 {
    let xb = unsafe { cxin(x, n, incx, C64) };
    call_f32("lind_cblas_scasum", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cdotu_sub(n: c_int, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, dotu: *mut c_void) {
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxin(y, n, incy, C64) };
    let db = unsafe { cxscalar_out(dotu, C64) };
    call("lind_cblas_cdotu_sub", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::Out { dst: db, len: OutLen::Cap }]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cdotc_sub(n: c_int, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, dotc: *mut c_void) {
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxin(y, n, incy, C64) };
    let db = unsafe { cxscalar_out(dotc, C64) };
    call("lind_cblas_cdotc_sub", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::Out { dst: db, len: OutLen::Cap }]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_caxpy(n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxout(y, n, incy, C64) }; // y := alpha*x + y  (read + write)
    call("lind_cblas_caxpy", &mut [Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_ccopy(n: c_int, x: *const c_void, incx: c_int, y: *mut c_void, incy: c_int) {
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxout(y, n, incy, C64) }; // strided output -> InOut (preserve gaps)
    call("lind_cblas_ccopy", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_cswap(n: c_int, x: *mut c_void, incx: c_int, y: *mut c_void, incy: c_int) {
    let xb = unsafe { cxout(x, n, incx, C64) };
    let yb = unsafe { cxout(y, n, incy, C64) };
    call("lind_cblas_cswap", &mut [Arg::I32(n), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_cscal(n: c_int, alpha: *const c_void, x: *mut c_void, incx: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let xb = unsafe { cxout(x, n, incx, C64) }; // x := alpha*x  (in/out)
    call("lind_cblas_cscal", &mut [Arg::I32(n), Arg::Buf(al), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_csscal(n: c_int, alpha: f32, x: *mut c_void, incx: c_int) {
    let xb = unsafe { cxout(x, n, incx, C64) }; // real scalar times complex vector
    call("lind_cblas_csscal", &mut [Arg::I32(n), Arg::F32(alpha), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

// --- double-complex (z) ------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn cblas_izamax(n: c_int, x: *const c_void, incx: c_int) -> usize {
    let xb = unsafe { cxin(x, n, incx, C128) };
    call("lind_cblas_izamax", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)]) as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dznrm2(n: c_int, x: *const c_void, incx: c_int) -> f64 {
    let xb = unsafe { cxin(x, n, incx, C128) };
    call_f64("lind_cblas_dznrm2", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_dzasum(n: c_int, x: *const c_void, incx: c_int) -> f64 {
    let xb = unsafe { cxin(x, n, incx, C128) };
    call_f64("lind_cblas_dzasum", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx)])
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zdotu_sub(n: c_int, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, dotu: *mut c_void) {
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxin(y, n, incy, C128) };
    let db = unsafe { cxscalar_out(dotu, C128) };
    call("lind_cblas_zdotu_sub", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::Out { dst: db, len: OutLen::Cap }]);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zdotc_sub(n: c_int, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, dotc: *mut c_void) {
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxin(y, n, incy, C128) };
    let db = unsafe { cxscalar_out(dotc, C128) };
    call("lind_cblas_zdotc_sub", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::Out { dst: db, len: OutLen::Cap }]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_zaxpy(n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxout(y, n, incy, C128) };
    call("lind_cblas_zaxpy", &mut [Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_zcopy(n: c_int, x: *const c_void, incx: c_int, y: *mut c_void, incy: c_int) {
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxout(y, n, incy, C128) };
    call("lind_cblas_zcopy", &mut [Arg::I32(n), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_zswap(n: c_int, x: *mut c_void, incx: c_int, y: *mut c_void, incy: c_int) {
    let xb = unsafe { cxout(x, n, incx, C128) };
    let yb = unsafe { cxout(y, n, incy, C128) };
    call("lind_cblas_zswap", &mut [Arg::I32(n), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_zscal(n: c_int, alpha: *const c_void, x: *mut c_void, incx: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_zscal", &mut [Arg::I32(n), Arg::Buf(al), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}

#[unsafe(no_mangle)]
pub extern "C" fn cblas_zdscal(n: c_int, alpha: f64, x: *mut c_void, incx: c_int) {
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_zdscal", &mut [Arg::I32(n), Arg::F64(alpha), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}


// ===================================================================================
// CBLAS complex level-2 (c/z). Element COUNTS reuse the real level-2 size helpers
// (gemat_elems / bandmat_elems / packed_elems / sqmat_elems); only the byte width
// (C64/C128) changes. New wrinkles vs the real level-2 sets:
//   * alpha/beta cross BY POINTER (Arg::Buf of one element) for gemv/gbmv, the Hermitian
//     mat-vecs (hemv/hbmv/hpmv), and the rank-1/2 updates (geru/gerc/her2/hpr2).
//   * her/hpr take a REAL alpha BY VALUE (F32 for c, F64 for z) — the only by-value scalar
//     in complex level-2 (the Hermitian diagonal stays real).
//   * ger splits into geru (unconjugated) and gerc (conjugated); both update A in place.
// A/AP is Buf for the mat-vecs, InOut for the rank updates; x is in/out for the triangular
// products/solves, else read-only; y (mat-vec output) is in/out.
// ===================================================================================

// --- single-complex (c) ------------------------------------------------------------

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cgemv(order: c_int, trans: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let ab = unsafe { cmat_in(a, gemat_elems(order, m, n, lda), C64) };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { cxin(x, lenx, incx, C64) };
    let yb = unsafe { cxout(y, leny, incy, C64) };
    call("lind_cblas_cgemv", &mut [Arg::I32(order), Arg::I32(trans), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cgbmv(order: c_int, trans: c_int, m: c_int, n: c_int, kl: c_int, ku: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let ab = unsafe { cmat_in(a, bandmat_elems(n, kl, lda), C64) };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { cxin(x, lenx, incx, C64) };
    let yb = unsafe { cxout(y, leny, incy, C64) };
    call("lind_cblas_cgbmv", &mut [Arg::I32(order), Arg::I32(trans), Arg::I32(m), Arg::I32(n), Arg::I32(kl), Arg::I32(ku), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_chemv(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let ab = unsafe { cmat_in(a, sqmat_elems(n, lda), C64) };
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxout(y, n, incy, C64) };
    call("lind_cblas_chemv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_chbmv(order: c_int, uplo: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let ab = unsafe { cmat_in(a, bandmat_elems(n, k, lda), C64) };
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxout(y, n, incy, C64) };
    call("lind_cblas_chbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_chpmv(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, ap: *const c_void, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let apb = unsafe { cmat_in(ap, packed_elems(n), C64) };
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxout(y, n, incy, C64) };
    call("lind_cblas_chpmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(apb), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctrmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, sqmat_elems(n, lda), C64) };
    let xb = unsafe { cxout(x, n, incx, C64) };
    call("lind_cblas_ctrmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctbmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, bandmat_elems(n, k, lda), C64) };
    let xb = unsafe { cxout(x, n, incx, C64) };
    call("lind_cblas_ctbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctpmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const c_void, x: *mut c_void, incx: c_int) {
    let apb = unsafe { cmat_in(ap, packed_elems(n), C64) };
    let xb = unsafe { cxout(x, n, incx, C64) };
    call("lind_cblas_ctpmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctrsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, sqmat_elems(n, lda), C64) };
    let xb = unsafe { cxout(x, n, incx, C64) };
    call("lind_cblas_ctrsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctbsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, bandmat_elems(n, k, lda), C64) };
    let xb = unsafe { cxout(x, n, incx, C64) };
    call("lind_cblas_ctbsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctpsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const c_void, x: *mut c_void, incx: c_int) {
    let apb = unsafe { cmat_in(ap, packed_elems(n), C64) };
    let xb = unsafe { cxout(x, n, incx, C64) };
    call("lind_cblas_ctpsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cgeru(order: c_int, m: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, a: *mut c_void, lda: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let xb = unsafe { cxin(x, m, incx, C64) };
    let yb = unsafe { cxin(y, n, incy, C64) };
    let ab = unsafe { cmat_out(a, gemat_elems(order, m, n, lda), C64) };
    call("lind_cblas_cgeru", &mut [Arg::I32(order), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cgerc(order: c_int, m: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, a: *mut c_void, lda: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let xb = unsafe { cxin(x, m, incx, C64) };
    let yb = unsafe { cxin(y, n, incy, C64) };
    let ab = unsafe { cmat_out(a, gemat_elems(order, m, n, lda), C64) };
    call("lind_cblas_cgerc", &mut [Arg::I32(order), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cher(order: c_int, uplo: c_int, n: c_int, alpha: f32, x: *const c_void, incx: c_int, a: *mut c_void, lda: c_int) {
    let xb = unsafe { cxin(x, n, incx, C64) };
    let ab = unsafe { cmat_out(a, sqmat_elems(n, lda), C64) };
    call("lind_cblas_cher", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_chpr(order: c_int, uplo: c_int, n: c_int, alpha: f32, x: *const c_void, incx: c_int, ap: *mut c_void) {
    let xb = unsafe { cxin(x, n, incx, C64) };
    let apb = unsafe { cmat_out(ap, packed_elems(n), C64) };
    call("lind_cblas_chpr", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F32(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cher2(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, a: *mut c_void, lda: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxin(y, n, incy, C64) };
    let ab = unsafe { cmat_out(a, sqmat_elems(n, lda), C64) };
    call("lind_cblas_cher2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_chpr2(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, ap: *mut c_void) {
    let al = unsafe { cxscalar(alpha, C64) };
    let xb = unsafe { cxin(x, n, incx, C64) };
    let yb = unsafe { cxin(y, n, incy, C64) };
    let apb = unsafe { cmat_out(ap, packed_elems(n), C64) };
    call("lind_cblas_chpr2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}

// --- double-complex (z) ------------------------------------------------------------

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zgemv(order: c_int, trans: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let ab = unsafe { cmat_in(a, gemat_elems(order, m, n, lda), C128) };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { cxin(x, lenx, incx, C128) };
    let yb = unsafe { cxout(y, leny, incy, C128) };
    call("lind_cblas_zgemv", &mut [Arg::I32(order), Arg::I32(trans), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zgbmv(order: c_int, trans: c_int, m: c_int, n: c_int, kl: c_int, ku: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let ab = unsafe { cmat_in(a, bandmat_elems(n, kl, lda), C128) };
    let (lenx, leny) = gemv_vec_lens(trans, m, n);
    let xb = unsafe { cxin(x, lenx, incx, C128) };
    let yb = unsafe { cxout(y, leny, incy, C128) };
    call("lind_cblas_zgbmv", &mut [Arg::I32(order), Arg::I32(trans), Arg::I32(m), Arg::I32(n), Arg::I32(kl), Arg::I32(ku), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zhemv(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let ab = unsafe { cmat_in(a, sqmat_elems(n, lda), C128) };
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxout(y, n, incy, C128) };
    call("lind_cblas_zhemv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zhbmv(order: c_int, uplo: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let ab = unsafe { cmat_in(a, bandmat_elems(n, k, lda), C128) };
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxout(y, n, incy, C128) };
    call("lind_cblas_zhbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zhpmv(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, ap: *const c_void, x: *const c_void, incx: c_int, beta: *const c_void, y: *mut c_void, incy: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let apb = unsafe { cmat_in(ap, packed_elems(n), C128) };
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxout(y, n, incy, C128) };
    call("lind_cblas_zhpmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(apb), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(be), Arg::InOut { dst: yb, len: OutLen::Cap }, Arg::I32(incy)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztrmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, sqmat_elems(n, lda), C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_ztrmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztbmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, bandmat_elems(n, k, lda), C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_ztbmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztpmv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const c_void, x: *mut c_void, incx: c_int) {
    let apb = unsafe { cmat_in(ap, packed_elems(n), C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_ztpmv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztrsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, sqmat_elems(n, lda), C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_ztrsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztbsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, k: c_int, a: *const c_void, lda: c_int, x: *mut c_void, incx: c_int) {
    let ab = unsafe { cmat_in(a, bandmat_elems(n, k, lda), C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_ztbsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::I32(k), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztpsv(order: c_int, uplo: c_int, trans: c_int, diag: c_int, n: c_int, ap: *const c_void, x: *mut c_void, incx: c_int) {
    let apb = unsafe { cmat_in(ap, packed_elems(n), C128) };
    let xb = unsafe { cxout(x, n, incx, C128) };
    call("lind_cblas_ztpsv", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(n), Arg::Buf(apb), Arg::InOut { dst: xb, len: OutLen::Cap }, Arg::I32(incx)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zgeru(order: c_int, m: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, a: *mut c_void, lda: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let xb = unsafe { cxin(x, m, incx, C128) };
    let yb = unsafe { cxin(y, n, incy, C128) };
    let ab = unsafe { cmat_out(a, gemat_elems(order, m, n, lda), C128) };
    call("lind_cblas_zgeru", &mut [Arg::I32(order), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zgerc(order: c_int, m: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, a: *mut c_void, lda: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let xb = unsafe { cxin(x, m, incx, C128) };
    let yb = unsafe { cxin(y, n, incy, C128) };
    let ab = unsafe { cmat_out(a, gemat_elems(order, m, n, lda), C128) };
    call("lind_cblas_zgerc", &mut [Arg::I32(order), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zher(order: c_int, uplo: c_int, n: c_int, alpha: f64, x: *const c_void, incx: c_int, a: *mut c_void, lda: c_int) {
    let xb = unsafe { cxin(x, n, incx, C128) };
    let ab = unsafe { cmat_out(a, sqmat_elems(n, lda), C128) };
    call("lind_cblas_zher", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
pub extern "C" fn cblas_zhpr(order: c_int, uplo: c_int, n: c_int, alpha: f64, x: *const c_void, incx: c_int, ap: *mut c_void) {
    let xb = unsafe { cxin(x, n, incx, C128) };
    let apb = unsafe { cmat_out(ap, packed_elems(n), C128) };
    call("lind_cblas_zhpr", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::F64(alpha), Arg::Buf(xb), Arg::I32(incx), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zher2(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, a: *mut c_void, lda: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxin(y, n, incy, C128) };
    let ab = unsafe { cmat_out(a, sqmat_elems(n, lda), C128) };
    call("lind_cblas_zher2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: ab, len: OutLen::Cap }, Arg::I32(lda)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zhpr2(order: c_int, uplo: c_int, n: c_int, alpha: *const c_void, x: *const c_void, incx: c_int, y: *const c_void, incy: c_int, ap: *mut c_void) {
    let al = unsafe { cxscalar(alpha, C128) };
    let xb = unsafe { cxin(x, n, incx, C128) };
    let yb = unsafe { cxin(y, n, incy, C128) };
    let apb = unsafe { cmat_out(ap, packed_elems(n), C128) };
    call("lind_cblas_zhpr2", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(n), Arg::Buf(al), Arg::Buf(xb), Arg::I32(incx), Arg::Buf(yb), Arg::I32(incy), Arg::InOut { dst: apb, len: OutLen::Cap }]);
}


// ===================================================================================
// CBLAS complex level-3 (c/z). Same shape rules as the real level-3 set: every operand
// is a full 2D matrix sized gemat_elems(order, R, C, ld); (R,C) via trans_dims (flip by
// trans) or square via side_dim. A/B are Buf; C is in/out (gemm/symm/hemm/syrk/herk/
// syr2k/her2k), B is in/out (trmm/trsm). Adds the complex-only hemm/herk/her2k. Scalars:
//   * alpha/beta by POINTER for gemm/symm/hemm/syrk/syr2k, and alpha for trmm/trsm.
//   * herk takes BOTH alpha and beta REAL by value (F32/F64).
//   * her2k takes a complex alpha (by ptr) but a REAL beta by value.
// ===================================================================================

// --- single-complex (c) ------------------------------------------------------------

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cgemm(order: c_int, transa: c_int, transb: c_int, m: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let (ar, ac) = trans_dims(transa, m, k);
    let (br, bc) = trans_dims(transb, k, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C64) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, br, bc, ldb), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, m, n, ldc), C64) };
    call("lind_cblas_cgemm", &mut [Arg::I32(order), Arg::I32(transa), Arg::I32(transb), Arg::I32(m), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_csymm(order: c_int, side: c_int, uplo: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C64) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, m, n, ldb), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, m, n, ldc), C64) };
    call("lind_cblas_csymm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_chemm(order: c_int, side: c_int, uplo: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C64) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, m, n, ldb), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, m, n, ldc), C64) };
    call("lind_cblas_chemm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_csyrk(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C64) };
    call("lind_cblas_csyrk", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cherk(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: f32, a: *const c_void, lda: c_int, beta: f32, c: *mut c_void, ldc: c_int) {
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C64) };
    call("lind_cblas_cherk", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::F32(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::F32(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_csyr2k(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let be = unsafe { cxscalar(beta, C64) };
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C64) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, ar, ac, ldb), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C64) };
    call("lind_cblas_csyr2k", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_cher2k(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: f32, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C64) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, ar, ac, ldb), C64) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C64) };
    call("lind_cblas_cher2k", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F32(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctrmm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *mut c_void, ldb: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C64) };
    let bb = unsafe { cmat_out(b, gemat_elems(order, m, n, ldb), C64) };
    call("lind_cblas_ctrmm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ctrsm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *mut c_void, ldb: c_int) {
    let al = unsafe { cxscalar(alpha, C64) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C64) };
    let bb = unsafe { cmat_out(b, gemat_elems(order, m, n, ldb), C64) };
    call("lind_cblas_ctrsm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}

// --- double-complex (z) ------------------------------------------------------------

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zgemm(order: c_int, transa: c_int, transb: c_int, m: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let (ar, ac) = trans_dims(transa, m, k);
    let (br, bc) = trans_dims(transb, k, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C128) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, br, bc, ldb), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, m, n, ldc), C128) };
    call("lind_cblas_zgemm", &mut [Arg::I32(order), Arg::I32(transa), Arg::I32(transb), Arg::I32(m), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zsymm(order: c_int, side: c_int, uplo: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C128) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, m, n, ldb), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, m, n, ldc), C128) };
    call("lind_cblas_zsymm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zhemm(order: c_int, side: c_int, uplo: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C128) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, m, n, ldb), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, m, n, ldc), C128) };
    call("lind_cblas_zhemm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zsyrk(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C128) };
    call("lind_cblas_zsyrk", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zherk(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: f64, a: *const c_void, lda: c_int, beta: f64, c: *mut c_void, ldc: c_int) {
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C128) };
    call("lind_cblas_zherk", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::F64(alpha), Arg::Buf(ab), Arg::I32(lda), Arg::F64(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zsyr2k(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: *const c_void, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let be = unsafe { cxscalar(beta, C128) };
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C128) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, ar, ac, ldb), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C128) };
    call("lind_cblas_zsyr2k", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::Buf(be), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_zher2k(order: c_int, uplo: c_int, trans: c_int, n: c_int, k: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *const c_void, ldb: c_int, beta: f64, c: *mut c_void, ldc: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let (ar, ac) = trans_dims(trans, n, k);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ar, ac, lda), C128) };
    let bb = unsafe { cmat_in(b, gemat_elems(order, ar, ac, ldb), C128) };
    let cb = unsafe { cmat_out(c, gemat_elems(order, n, n, ldc), C128) };
    call("lind_cblas_zher2k", &mut [Arg::I32(order), Arg::I32(uplo), Arg::I32(trans), Arg::I32(n), Arg::I32(k), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::Buf(bb), Arg::I32(ldb), Arg::F64(beta), Arg::InOut { dst: cb, len: OutLen::Cap }, Arg::I32(ldc)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztrmm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *mut c_void, ldb: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C128) };
    let bb = unsafe { cmat_out(b, gemat_elems(order, m, n, ldb), C128) };
    call("lind_cblas_ztrmm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cblas_ztrsm(order: c_int, side: c_int, uplo: c_int, trans: c_int, diag: c_int, m: c_int, n: c_int, alpha: *const c_void, a: *const c_void, lda: c_int, b: *mut c_void, ldb: c_int) {
    let al = unsafe { cxscalar(alpha, C128) };
    let ad = side_dim(side, m, n);
    let ab = unsafe { cmat_in(a, gemat_elems(order, ad, ad, lda), C128) };
    let bb = unsafe { cmat_out(b, gemat_elems(order, m, n, ldb), C128) };
    call("lind_cblas_ztrsm", &mut [Arg::I32(order), Arg::I32(side), Arg::I32(uplo), Arg::I32(trans), Arg::I32(diag), Arg::I32(m), Arg::I32(n), Arg::Buf(al), Arg::Buf(ab), Arg::I32(lda), Arg::InOut { dst: bb, len: OutLen::Cap }, Arg::I32(ldb)]);
}

// ===================================================================================
// Fortran-ABI (BLASFUNC) forwarders for the utest suite. Unlike the reference CBLAS
// ctest path, utest calls the Fortran symbols (daxpy_, dscal_, ...): EVERY argument is
// a pointer, there is no `order` (BLAS is implicitly column-major), and the integer
// i?amax result is 1-based. Each forwarder just derefs its pointer args and calls the
// already-sandboxed cblas_* above — so utest exercises the very same guest code.
//
// This is the STANDARD level-1 set only. Routines needing more work stay on the native
// libopenblas.a fallback in the harness for now: complex dot (cdotu_/zdotu_ — fragile
// complex-return ABI), dsdot/sdsdot (need a cblas_dsdot), rotmg, the char-flag level-2/3
// forwarders (gemv_/gemm_ — need 'N'/'T' -> enum translation), and the OpenBLAS
// extensions (amax/amin/axpby/ismin — need extension cblas_* wrappers).
//
// (drot_/drotm_/srot_/srotm_ are already defined above for the ctest level-1 driver.)
// ===================================================================================

// --- real single/double: axpy, copy, swap, scal ------------------------------------
#[unsafe(no_mangle)]
pub extern "C" fn saxpy_(n: *const c_int, alpha: *const f32, x: *const f32, incx: *const c_int, y: *mut f32, incy: *const c_int) {
    unsafe { cblas_saxpy(*n, *alpha, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn daxpy_(n: *const c_int, alpha: *const f64, x: *const f64, incx: *const c_int, y: *mut f64, incy: *const c_int) {
    unsafe { cblas_daxpy(*n, *alpha, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn scopy_(n: *const c_int, x: *const f32, incx: *const c_int, y: *mut f32, incy: *const c_int) {
    unsafe { cblas_scopy(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dcopy_(n: *const c_int, x: *const f64, incx: *const c_int, y: *mut f64, incy: *const c_int) {
    unsafe { cblas_dcopy(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn sswap_(n: *const c_int, x: *mut f32, incx: *const c_int, y: *mut f32, incy: *const c_int) {
    unsafe { cblas_sswap(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dswap_(n: *const c_int, x: *mut f64, incx: *const c_int, y: *mut f64, incy: *const c_int) {
    unsafe { cblas_dswap(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn sscal_(n: *const c_int, alpha: *const f32, x: *mut f32, incx: *const c_int) {
    unsafe { cblas_sscal(*n, *alpha, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dscal_(n: *const c_int, alpha: *const f64, x: *mut f64, incx: *const c_int) {
    unsafe { cblas_dscal(*n, *alpha, x, *incx) }
}

// --- real single/double: dot / nrm2 / asum (scalar returns) ------------------------
#[unsafe(no_mangle)]
pub extern "C" fn sdot_(n: *const c_int, x: *const f32, incx: *const c_int, y: *const f32, incy: *const c_int) -> f32 {
    unsafe { cblas_sdot(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn ddot_(n: *const c_int, x: *const f64, incx: *const c_int, y: *const f64, incy: *const c_int) -> f64 {
    unsafe { cblas_ddot(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn snrm2_(n: *const c_int, x: *const f32, incx: *const c_int) -> f32 {
    unsafe { cblas_snrm2(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dnrm2_(n: *const c_int, x: *const f64, incx: *const c_int) -> f64 {
    unsafe { cblas_dnrm2(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn sasum_(n: *const c_int, x: *const f32, incx: *const c_int) -> f32 {
    unsafe { cblas_sasum(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dasum_(n: *const c_int, x: *const f64, incx: *const c_int) -> f64 {
    unsafe { cblas_dasum(*n, x, *incx) }
}

/// Fortran i?amax: returns a 1-based index, or 0 when n < 1 (cblas is 0-based).
fn famax(zero_based: usize, n: c_int) -> c_int {
    if n < 1 { 0 } else { zero_based as c_int + 1 }
}
#[unsafe(no_mangle)]
pub extern "C" fn isamax_(n: *const c_int, x: *const f32, incx: *const c_int) -> c_int {
    unsafe { famax(cblas_isamax(*n, x, *incx), *n) }
}
#[unsafe(no_mangle)]
pub extern "C" fn idamax_(n: *const c_int, x: *const f64, incx: *const c_int) -> c_int {
    unsafe { famax(cblas_idamax(*n, x, *incx), *n) }
}

// --- complex single/double: axpy, copy, swap, scal ---------------------------------
#[unsafe(no_mangle)]
pub extern "C" fn caxpy_(n: *const c_int, alpha: *const c_void, x: *const c_void, incx: *const c_int, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_caxpy(*n, alpha, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn zaxpy_(n: *const c_int, alpha: *const c_void, x: *const c_void, incx: *const c_int, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_zaxpy(*n, alpha, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn ccopy_(n: *const c_int, x: *const c_void, incx: *const c_int, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_ccopy(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn zcopy_(n: *const c_int, x: *const c_void, incx: *const c_int, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_zcopy(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn cswap_(n: *const c_int, x: *mut c_void, incx: *const c_int, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_cswap(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn zswap_(n: *const c_int, x: *mut c_void, incx: *const c_int, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_zswap(*n, x, *incx, y, *incy) }
}
#[unsafe(no_mangle)]
pub extern "C" fn cscal_(n: *const c_int, alpha: *const c_void, x: *mut c_void, incx: *const c_int) {
    unsafe { cblas_cscal(*n, alpha, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn zscal_(n: *const c_int, alpha: *const c_void, x: *mut c_void, incx: *const c_int) {
    unsafe { cblas_zscal(*n, alpha, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn csscal_(n: *const c_int, alpha: *const f32, x: *mut c_void, incx: *const c_int) {
    unsafe { cblas_csscal(*n, *alpha, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn zdscal_(n: *const c_int, alpha: *const f64, x: *mut c_void, incx: *const c_int) {
    unsafe { cblas_zdscal(*n, *alpha, x, *incx) }
}

// --- complex single/double: nrm2 / asum (real returns) / iamax ---------------------
#[unsafe(no_mangle)]
pub extern "C" fn scnrm2_(n: *const c_int, x: *const c_void, incx: *const c_int) -> f32 {
    unsafe { cblas_scnrm2(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dznrm2_(n: *const c_int, x: *const c_void, incx: *const c_int) -> f64 {
    unsafe { cblas_dznrm2(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn scasum_(n: *const c_int, x: *const c_void, incx: *const c_int) -> f32 {
    unsafe { cblas_scasum(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn dzasum_(n: *const c_int, x: *const c_void, incx: *const c_int) -> f64 {
    unsafe { cblas_dzasum(*n, x, *incx) }
}
#[unsafe(no_mangle)]
pub extern "C" fn icamax_(n: *const c_int, x: *const c_void, incx: *const c_int) -> c_int {
    unsafe { famax(cblas_icamax(*n, x, *incx), *n) }
}
#[unsafe(no_mangle)]
pub extern "C" fn izamax_(n: *const c_int, x: *const c_void, incx: *const c_int) -> c_int {
    unsafe { famax(cblas_izamax(*n, x, *incx), *n) }
}


// --- level-2 Fortran forwarders: gemv ----------------------------------------------
// The Fortran ABI differs from level-1 in two ways handled here: the transpose flag is a
// CHARACTER ('N'/'T'/'C', any case) rather than a CBLAS enum int, and there is no `order`
// (Fortran BLAS is column-major). We translate the char and pass CblasColMajor, then reuse
// cblas_?gemv — whose sizing (gemat_elems = lda*n for col-major, gemv_vec_lens) already
// matches. No shim change: lind_cblas_?gemv already exists from the ctest level-2 work.
// (OpenBLAS's own C Fortran interface takes the char by pointer with no hidden length arg,
// and utest calls it the same way, so the forwarder takes just `*const c_char`.)

use core::ffi::c_char;

/// Fortran BLAS trans flag ('N'/'T'/'C', any case) -> CBLAS enum.
fn trans_enum(c: c_char) -> c_int {
    match (c as u8).to_ascii_uppercase() {
        b'T' => 112,         // CblasTrans
        b'C' => 113,         // CblasConjTrans
        _ => CBLAS_NO_TRANS, // 'N'
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn sgemv_(trans: *const c_char, m: *const c_int, n: *const c_int, alpha: *const f32, a: *const f32, lda: *const c_int, x: *const f32, incx: *const c_int, beta: *const f32, y: *mut f32, incy: *const c_int) {
    unsafe { cblas_sgemv(CBLAS_COL_MAJOR, trans_enum(*trans), *m, *n, *alpha, a, *lda, x, *incx, *beta, y, *incy) }
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dgemv_(trans: *const c_char, m: *const c_int, n: *const c_int, alpha: *const f64, a: *const f64, lda: *const c_int, x: *const f64, incx: *const c_int, beta: *const f64, y: *mut f64, incy: *const c_int) {
    unsafe { cblas_dgemv(CBLAS_COL_MAJOR, trans_enum(*trans), *m, *n, *alpha, a, *lda, x, *incx, *beta, y, *incy) }
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn cgemv_(trans: *const c_char, m: *const c_int, n: *const c_int, alpha: *const c_void, a: *const c_void, lda: *const c_int, x: *const c_void, incx: *const c_int, beta: *const c_void, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_cgemv(CBLAS_COL_MAJOR, trans_enum(*trans), *m, *n, alpha, a, *lda, x, *incx, beta, y, *incy) }
}
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn zgemv_(trans: *const c_char, m: *const c_int, n: *const c_int, alpha: *const c_void, a: *const c_void, lda: *const c_int, x: *const c_void, incx: *const c_int, beta: *const c_void, y: *mut c_void, incy: *const c_int) {
    unsafe { cblas_zgemv(CBLAS_COL_MAJOR, trans_enum(*trans), *m, *n, alpha, a, *lda, x, *incx, beta, y, *incy) }
}

