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
const _CBLAS_COL_MAJOR: c_int = 102;
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
