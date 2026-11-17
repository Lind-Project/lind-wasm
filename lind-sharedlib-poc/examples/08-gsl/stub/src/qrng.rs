// Hand-written (Stage I). Quasi-random (low-discrepancy) sequences (gsl_qrng) — no callbacks.
// Uses call/call_handle/Arg/OutLen from lib.rs; HashMap from std; c_int/c_uint/c_void from core::ffi.
//
// Type-pointer constructor + opaque handle (class 32). gsl_qrng_get(q, x[]) writes `dimension`
// doubles but does not take the dimension as an argument, so the stub remembers it per generator
// (bound at alloc) to size the output buffer — the same per-handle bookkeeping used by the ODE
// driver and the 1-D solvers.


const CLASS_QRNG: u32 = 32;

// qrng token -> dimension (for sizing gsl_qrng_get's output)
static QRNG_DIM: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
fn qrng_dim() -> &'static Mutex<HashMap<u64, usize>> {
    QRNG_DIM.get_or_init(|| Mutex::new(HashMap::new()))
}

// type descriptors
#[unsafe(no_mangle)] pub static gsl_qrng_niederreiter_2: usize = 1;
#[unsafe(no_mangle)] pub static gsl_qrng_sobol: usize = 2;
#[unsafe(no_mangle)] pub static gsl_qrng_halton: usize = 3;
#[unsafe(no_mangle)] pub static gsl_qrng_reversehalton: usize = 4;

#[unsafe(no_mangle)]
pub extern "C" fn gsl_qrng_alloc(t: *const c_void, d: c_uint) -> *mut c_void {
    let q = call_handle(
        "lind_gsl_qrng_alloc",
        &mut [Arg::I32(t as usize as i64 as i32), Arg::I32(d as i32)],
        CLASS_QRNG,
    );
    qrng_dim().lock().unwrap().insert(q, d as usize);
    q as *mut c_void
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_qrng_free(q: *mut c_void) {
    qrng_dim().lock().unwrap().remove(&(q as u64));
    let _ = call("lind_gsl_qrng_free", &mut [Arg::Handle { class: CLASS_QRNG, token: q as u64, release: true }]);
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_qrng_init(q: *mut c_void) {
    let _ = call("lind_gsl_qrng_init", &mut [Arg::Handle { class: CLASS_QRNG, token: q as u64, release: false }]);
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_qrng_get(q: *mut c_void, x: *mut f64) -> c_int {
    let d = qrng_dim().lock().unwrap().get(&(q as u64)).copied().unwrap_or(0);
    if d == 0 {
        return -1;
    }
    let xb = unsafe { core::slice::from_raw_parts_mut(x as *mut u8, d * 8) };
    call(
        "lind_gsl_qrng_get",
        &mut [Arg::Handle { class: CLASS_QRNG, token: q as u64, release: false }, Arg::Out { dst: xb, len: OutLen::Cap }],
    ) as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn gsl_qrng_memcpy(dest: *mut c_void, src: *mut c_void) -> c_int {
    call(
        "lind_gsl_qrng_memcpy",
        &mut [
            Arg::Handle { class: CLASS_QRNG, token: dest as u64, release: false },
            Arg::Handle { class: CLASS_QRNG, token: src as u64, release: false },
        ],
    ) as c_int
}
