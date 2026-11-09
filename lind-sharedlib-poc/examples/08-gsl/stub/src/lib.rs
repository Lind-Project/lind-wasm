// GSL stub — the native .so whose exported gsl_* symbols run their real implementations
// inside the lind sandbox. The test suite / app links this like normal GSL (-lgsl) and
// calls these symbols unaware the work happens in a wasm guest. First call brings up the
// runtime and instantiates the GSL guest; later calls reuse it.
//
// First targets: gsl_poly_solve_quadratic / gsl_poly_solve_cubic — scalars in, out-pointers,
// int return. (gsl_poly_eval is INLINE in GSL's headers, so it compiles into the caller and
// can't be sandboxed — nothing to forward.)

use std::sync::{Mutex, OnceLock};

use core::ffi::{c_int, c_uint, c_void};

use lind_boot::{Arg, CliOptions, OutLen, SandboxedLib, init_sandboxed_lib};

static LIB: OnceLock<Mutex<SandboxedLib>> = OnceLock::new();

fn module_path() -> String {
    std::env::var("LIND_MODULE").unwrap_or_else(|_| "gsl_lind_shim.cwasm".to_string())
}

fn lib() -> &'static Mutex<SandboxedLib> {
    LIB.get_or_init(|| {
        let cli = CliOptions::for_sandboxed_lib(module_path());
        let sandboxed_lib = init_sandboxed_lib(cli)
            .unwrap_or_else(|e| panic!("lind sandboxed-lib init failed: {e:?}"));
        Mutex::new(sandboxed_lib)
    })
}

/// Print `[sandbox] <name>` to stderr when LIND_TRACE is set — a per-call audit trail
/// proving the symbol ran through the guest (not the native fallback).
fn trace(name: &str) {
    if std::env::var_os("LIND_TRACE").is_some() {
        eprintln!("[sandbox] {name}");
    }
}

/// Call a guest export; returns the guest's integer return value.
fn call(name: &str, args: &mut [Arg]) -> i64 {
    trace(name);
    lib()
        .lock()
        .unwrap()
        .call(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

/// Call a guest export that returns `double` (used by the generated statistics wrappers).
fn call_f64(name: &str, args: &mut [Arg]) -> f64 {
    trace(name);
    lib()
        .lock()
        .unwrap()
        .call_f64(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

/// Call a guest export that returns `float` (used by the generated `_float` handle modules).
fn call_f32(name: &str, args: &mut [Arg]) -> f32 {
    trace(name);
    lib()
        .lock()
        .unwrap()
        .call_f32(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

/// Call a guest **constructor** that returns a pointer to an opaque object; register the
/// returned guest offset as a handle of `class` and hand the caller the opaque token.
fn call_handle(name: &str, args: &mut [Arg], class: u32) -> u64 {
    trace(name);
    lib()
        .lock()
        .unwrap()
        .call_handle(name, args, class)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

// ---------------------------------------------------------------------------
// gsl_sort_vector — kept hand-written: it belongs to the SORT module, not the vector
// module. The gsl_vector object itself is now owned by the GENERATED vector module
// (see gen_gsl_handle.py), which defines the CLASS_VECTOR constant used here. The opaque
// handle still only works while the app reaches the vector through wrapped FUNCTIONS
// (gsl_vector_get/set are inline under HAVE_INLINE — the app must be built without it).
// ---------------------------------------------------------------------------
#[unsafe(no_mangle)]
pub extern "C" fn gsl_sort_vector(v: *mut c_void) {
    call(
        "lind_gsl_sort_vector",
        &mut [Arg::Handle { class: CLASS_VECTOR, token: v as u64, release: false }],
    );
}

/// Write an out-double back to a caller pointer (if non-null). The roots are only written by
/// GSL when they exist; for the no-real-root case the caller ignores them, so a copy-back of
/// whatever the guest left is harmless.
unsafe fn put(p: *mut f64, bytes: [u8; 8]) {
    if !p.is_null() {
        unsafe { *p = f64::from_le_bytes(bytes) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_solve_quadratic(
    a: f64, b: f64, c: f64, x0: *mut f64, x1: *mut f64,
) -> c_int {
    let mut o0 = [0u8; 8];
    let mut o1 = [0u8; 8];
    let n = call(
        "lind_gsl_poly_solve_quadratic",
        &mut [
            Arg::F64(a),
            Arg::F64(b),
            Arg::F64(c),
            Arg::Out { dst: &mut o0, len: OutLen::Cap },
            Arg::Out { dst: &mut o1, len: OutLen::Cap },
        ],
    ) as c_int;
    unsafe {
        put(x0, o0);
        put(x1, o1);
    }
    n
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_solve_cubic(
    a: f64, b: f64, c: f64, x0: *mut f64, x1: *mut f64, x2: *mut f64,
) -> c_int {
    let mut o0 = [0u8; 8];
    let mut o1 = [0u8; 8];
    let mut o2 = [0u8; 8];
    let n = call(
        "lind_gsl_poly_solve_cubic",
        &mut [
            Arg::F64(a),
            Arg::F64(b),
            Arg::F64(c),
            Arg::Out { dst: &mut o0, len: OutLen::Cap },
            Arg::Out { dst: &mut o1, len: OutLen::Cap },
            Arg::Out { dst: &mut o2, len: OutLen::Cap },
        ],
    ) as c_int;
    unsafe {
        put(x0, o0);
        put(x1, o1);
        put(x2, o2);
    }
    n
}

/// gsl_poly_eval_derivs(c[lenc], lenc, x, res[lenres], lenres): evaluate the polynomial with
/// coefficients `c` and its derivatives at `x`, writing lenres results into `res`. First GSL
/// function with an input array AND an output array:
///   - `c`   -> Arg::Buf  (copy lenc doubles INTO the guest)
///   - `res` -> Arg::Out  (guest writes lenres doubles; copied straight back into the caller's
///              buffer — we view res's bytes directly as the Out dst, so no extra copy)
/// GSL fully fills res[0..lenres], so Out (not InOut) is safe.
#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_eval_derivs(
    c: *const f64, lenc: usize, x: f64, res: *mut f64, lenres: usize,
) -> c_int {
    let cb = unsafe { core::slice::from_raw_parts(c as *const u8, lenc.saturating_mul(8)) };
    let resb = unsafe { core::slice::from_raw_parts_mut(res as *mut u8, lenres.saturating_mul(8)) };
    call(
        "lind_gsl_poly_eval_derivs",
        &mut [
            Arg::Buf(cb),
            Arg::USize(lenc),
            Arg::F64(x),
            Arg::Out { dst: resb, len: OutLen::Cap },
            Arg::USize(lenres),
        ],
    ) as c_int
}

// ---------------------------------------------------------------------------
// Tier-1 tail: hand-written wrappers for the irregular flat functions the generators
// skip — polynomial divided differences, the array-valued randist pdfs, and the
// special-function `_e` forms with a second (sign) output. All use existing Arg kinds.
// ---------------------------------------------------------------------------
use core::slice::{from_raw_parts, from_raw_parts_mut};

// --- polynomial divided differences (poly/dd.c) ---
#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_dd_init(dd: *mut f64, xa: *const f64, ya: *const f64, size: usize) -> c_int {
    let dd_b = unsafe { from_raw_parts_mut(dd as *mut u8, size * 8) };
    let xa_b = unsafe { from_raw_parts(xa as *const u8, size * 8) };
    let ya_b = unsafe { from_raw_parts(ya as *const u8, size * 8) };
    call("lind_gsl_poly_dd_init", &mut [
        Arg::Out { dst: dd_b, len: OutLen::Cap }, Arg::Buf(xa_b), Arg::Buf(ya_b), Arg::USize(size),
    ]) as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_dd_eval(dd: *const f64, xa: *const f64, size: usize, x: f64) -> f64 {
    let dd_b = unsafe { from_raw_parts(dd as *const u8, size * 8) };
    let xa_b = unsafe { from_raw_parts(xa as *const u8, size * 8) };
    call_f64("lind_gsl_poly_dd_eval", &mut [Arg::Buf(dd_b), Arg::Buf(xa_b), Arg::USize(size), Arg::F64(x)])
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_dd_taylor(
    c: *mut f64, xp: f64, dd: *const f64, xa: *const f64, size: usize, w: *mut f64,
) -> c_int {
    let c_b = unsafe { from_raw_parts_mut(c as *mut u8, size * 8) };
    let dd_b = unsafe { from_raw_parts(dd as *const u8, size * 8) };
    let xa_b = unsafe { from_raw_parts(xa as *const u8, size * 8) };
    let w_b = unsafe { from_raw_parts_mut(w as *mut u8, size * 8) }; // scratch; copy-back harmless
    call("lind_gsl_poly_dd_taylor", &mut [
        Arg::Out { dst: c_b, len: OutLen::Cap }, Arg::F64(xp), Arg::Buf(dd_b), Arg::Buf(xa_b),
        Arg::USize(size), Arg::Out { dst: w_b, len: OutLen::Cap },
    ]) as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_poly_dd_hermite_init(
    dd: *mut f64, z: *mut f64, xa: *const f64, ya: *const f64, dya: *const f64, size: usize,
) -> c_int {
    // Hermite doubles the node set: dd and z hold 2*size entries.
    let dd_b = unsafe { from_raw_parts_mut(dd as *mut u8, 2 * size * 8) };
    let z_b = unsafe { from_raw_parts_mut(z as *mut u8, 2 * size * 8) };
    let xa_b = unsafe { from_raw_parts(xa as *const u8, size * 8) };
    let ya_b = unsafe { from_raw_parts(ya as *const u8, size * 8) };
    let dya_b = unsafe { from_raw_parts(dya as *const u8, size * 8) };
    call("lind_gsl_poly_dd_hermite_init", &mut [
        Arg::Out { dst: dd_b, len: OutLen::Cap }, Arg::Out { dst: z_b, len: OutLen::Cap },
        Arg::Buf(xa_b), Arg::Buf(ya_b), Arg::Buf(dya_b), Arg::USize(size),
    ]) as c_int
}

// --- array-valued randist pdfs: dirichlet(K doubles×2), multinomial(K doubles + K uints) ---
#[unsafe(no_mangle)]
pub extern "C" fn gsl_ran_dirichlet_pdf(k: usize, alpha: *const f64, theta: *const f64) -> f64 {
    let a = unsafe { from_raw_parts(alpha as *const u8, k * 8) };
    let t = unsafe { from_raw_parts(theta as *const u8, k * 8) };
    call_f64("lind_gsl_ran_dirichlet_pdf", &mut [Arg::USize(k), Arg::Buf(a), Arg::Buf(t)])
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_ran_dirichlet_lnpdf(k: usize, alpha: *const f64, theta: *const f64) -> f64 {
    let a = unsafe { from_raw_parts(alpha as *const u8, k * 8) };
    let t = unsafe { from_raw_parts(theta as *const u8, k * 8) };
    call_f64("lind_gsl_ran_dirichlet_lnpdf", &mut [Arg::USize(k), Arg::Buf(a), Arg::Buf(t)])
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_ran_multinomial_pdf(k: usize, p: *const f64, n: *const c_uint) -> f64 {
    let pb = unsafe { from_raw_parts(p as *const u8, k * 8) };
    let nb = unsafe { from_raw_parts(n as *const u8, k * 4) };
    call_f64("lind_gsl_ran_multinomial_pdf", &mut [Arg::USize(k), Arg::Buf(pb), Arg::Buf(nb)])
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_ran_multinomial_lnpdf(k: usize, p: *const f64, n: *const c_uint) -> f64 {
    let pb = unsafe { from_raw_parts(p as *const u8, k * 8) };
    let nb = unsafe { from_raw_parts(n as *const u8, k * 4) };
    call_f64("lind_gsl_ran_multinomial_lnpdf", &mut [Arg::USize(k), Arg::Buf(pb), Arg::Buf(nb)])
}

// --- special-function `_e` forms with a second (sign) output: result[16] + sgn[8] ---
fn call_sgn_e(name: &str, scalars: &[f64], result: *mut c_void, sgn: *mut f64) -> c_int {
    let mut r = [0u8; 16];
    let mut s = [0u8; 8];
    // Scope the args so their mutable borrows of r/s end before we read them back.
    let st = {
        let mut args: Vec<Arg> = scalars.iter().map(|&v| Arg::F64(v)).collect();
        args.push(Arg::Out { dst: &mut r, len: OutLen::Cap });
        args.push(Arg::Out { dst: &mut s, len: OutLen::Cap });
        call(name, &mut args) as c_int
    };
    if !result.is_null() {
        unsafe { core::ptr::copy_nonoverlapping(r.as_ptr(), result as *mut u8, 16) };
    }
    unsafe { put(sgn, s) };
    st
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_sf_lngamma_sgn_e(x: f64, result: *mut c_void, sgn: *mut f64) -> c_int {
    call_sgn_e("lind_gsl_sf_lngamma_sgn_e", &[x], result, sgn)
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_sf_lnpoch_sgn_e(a: f64, x: f64, result: *mut c_void, sgn: *mut f64) -> c_int {
    call_sgn_e("lind_gsl_sf_lnpoch_sgn_e", &[a, x], result, sgn)
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_sf_lnbeta_sgn_e(x: f64, y: f64, result: *mut c_void, sgn: *mut f64) -> c_int {
    call_sgn_e("lind_gsl_sf_lnbeta_sgn_e", &[x, y], result, sgn)
}

// GSL sort module — generated marshalling (see gen_gsl_sort.py).
include!("sort_generated.rs");

// GSL statistics module — generated marshalling (see gen_gsl_stats.py).
include!("stats_generated.rs");

// GSL scalar modules (cdf / randist pdf / sys) — generated marshalling (see gen_gsl_scalar.py).
include!("scalar_generated.rs");

// GSL special-function `_e` forms (gsl_sf_*_e, result-struct out) — see gen_gsl_sf_result.py.
include!("sf_result_generated.rs");

// GSL opaque-handle modules (Tier 2) — generated marshalling (see gen_gsl_handle.py).
include!("vector_generated.rs"); // defines CLASS_VECTOR, used by gsl_sort_vector above
include!("vector_float_generated.rs");
include!("vector_int_generated.rs");
include!("vector_uint_generated.rs");
include!("matrix_generated.rs");
include!("matrix_float_generated.rs");
include!("matrix_int_generated.rs");
include!("matrix_uint_generated.rs");
include!("permutation_generated.rs");
include!("combination_generated.rs");
include!("histogram_generated.rs");
include!("linalg_generated.rs"); // free functions over matrix/permutation/vector handles
include!("eigen_symm_ws_generated.rs");  // eigen workspaces (opaque handles)
include!("eigen_symmv_ws_generated.rs");
include!("eigen_generated.rs");           // free-function solvers (matrix+vector+workspace)
include!("multifit_linear_ws_generated.rs");
include!("multifit_generated.rs");        // linear least-squares (workspace + matrix/vector)
