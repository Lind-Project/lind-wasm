// GSL stub — the native .so whose exported gsl_* symbols run their real implementations
// inside the lind sandbox. The test suite / app links this like normal GSL (-lgsl) and
// calls these symbols unaware the work happens in a wasm guest. First call brings up the
// runtime and instantiates the GSL guest; later calls reuse it.
//
// First targets: gsl_poly_solve_quadratic / gsl_poly_solve_cubic — scalars in, out-pointers,
// int return. (gsl_poly_eval is INLINE in GSL's headers, so it compiles into the caller and
// can't be sandboxed — nothing to forward.)

use std::sync::{Mutex, OnceLock};

use core::ffi::{c_int, c_void};

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

/// Call a guest export; returns the guest's integer return value.
fn call(name: &str, args: &mut [Arg]) -> i64 {
    lib()
        .lock()
        .unwrap()
        .call(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
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

// GSL sort module — generated marshalling (see gen_gsl_sort.py).
include!("sort_generated.rs");
