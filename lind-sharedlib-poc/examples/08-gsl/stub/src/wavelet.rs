// Hand-written (Stage F). Wavelet transforms — no callbacks.
// Uses call/call_handle/Arg/OutLen from lib.rs; c_int/c_void from core::ffi.
//
// Combines the three proven non-callback mechanisms: a library-owned TYPE descriptor
// (gsl_wavelet_daubechies, ...) resolved to an index, opaque handles (the wavelet and its
// workspace), and an in-place array transform (Arg::InOut). No engine change.

const CLASS_WAVELET: u32 = 29;
const CLASS_WAVELET_WS: u32 = 30;

// library-owned wavelet-type descriptors — value is the index the shim resolves.
#[unsafe(no_mangle)] pub static gsl_wavelet_daubechies: usize = 1;
#[unsafe(no_mangle)] pub static gsl_wavelet_daubechies_centered: usize = 2;
#[unsafe(no_mangle)] pub static gsl_wavelet_haar: usize = 3;
#[unsafe(no_mangle)] pub static gsl_wavelet_haar_centered: usize = 4;
#[unsafe(no_mangle)] pub static gsl_wavelet_bspline: usize = 5;
#[unsafe(no_mangle)] pub static gsl_wavelet_bspline_centered: usize = 6;

#[unsafe(no_mangle)]
pub extern "C" fn gsl_wavelet_alloc(t: *const c_void, k: usize) -> *mut c_void {
    call_handle("lind_gsl_wavelet_alloc", &mut [Arg::I32(t as usize as i64 as i32), Arg::USize(k)], CLASS_WAVELET) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_wavelet_free(w: *mut c_void) {
    let _ = call("lind_gsl_wavelet_free", &mut [Arg::Handle { class: CLASS_WAVELET, token: w as u64, release: true }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_wavelet_workspace_alloc(n: usize) -> *mut c_void {
    call_handle("lind_gsl_wavelet_workspace_alloc", &mut [Arg::USize(n)], CLASS_WAVELET_WS) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_wavelet_workspace_free(w: *mut c_void) {
    let _ = call("lind_gsl_wavelet_workspace_free", &mut [Arg::Handle { class: CLASS_WAVELET_WS, token: w as u64, release: true }]);
}

macro_rules! wavelet_transform {
    ($name:ident, $export:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(w: *mut c_void, data: *mut f64, stride: usize, n: usize, work: *mut c_void) -> c_int {
            if n == 0 {
                return 0;
            }
            let span = n.saturating_sub(1) * stride + 1; // reals the guest touches
            let b = unsafe { core::slice::from_raw_parts_mut(data as *mut u8, span * 8) };
            call(
                $export,
                &mut [
                    Arg::Handle { class: CLASS_WAVELET, token: w as u64, release: false },
                    Arg::InOut { dst: b, len: OutLen::Cap },
                    Arg::USize(stride),
                    Arg::USize(n),
                    Arg::Handle { class: CLASS_WAVELET_WS, token: work as u64, release: false },
                ],
            ) as c_int
        }
    };
}

wavelet_transform!(gsl_wavelet_transform_forward, "lind_gsl_wavelet_transform_forward");
wavelet_transform!(gsl_wavelet_transform_inverse, "lind_gsl_wavelet_transform_inverse");
