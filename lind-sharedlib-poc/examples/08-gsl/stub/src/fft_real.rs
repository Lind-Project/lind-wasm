// Hand-written (Stage C2). Real / halfcomplex MIXED-RADIX FFT (arbitrary n).
// Uses call/call_handle/Arg/OutLen from lib.rs; span_doubles from fft.rs (Stage C);
// c_int/c_void from core::ffi.
//
// Same shape as Stage C's complex mixed-radix: in-place transform (Arg::InOut, n reals) + two
// opaque handles. The twist is three new handle classes — a real wavetable, a halfcomplex
// wavetable, and a SHARED real workspace used by both. unpack expands a packed coefficient array
// (n reals) into a full complex array (2n doubles): a Buf in + an Out back.

const CLASS_FFT_REAL_WAVETABLE: u32 = 25;
const CLASS_FFT_HC_WAVETABLE: u32 = 26;
const CLASS_FFT_REAL_WORKSPACE: u32 = 27;

// ---- wavetables + shared workspace (opaque handles) -------------------------
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_real_wavetable_alloc(n: usize) -> *mut c_void {
    call_handle("lind_gsl_fft_real_wavetable_alloc", &mut [Arg::USize(n)], CLASS_FFT_REAL_WAVETABLE) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_real_wavetable_free(w: *mut c_void) {
    let _ = call("lind_gsl_fft_real_wavetable_free", &mut [Arg::Handle { class: CLASS_FFT_REAL_WAVETABLE, token: w as u64, release: true }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_halfcomplex_wavetable_alloc(n: usize) -> *mut c_void {
    call_handle("lind_gsl_fft_halfcomplex_wavetable_alloc", &mut [Arg::USize(n)], CLASS_FFT_HC_WAVETABLE) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_halfcomplex_wavetable_free(w: *mut c_void) {
    let _ = call("lind_gsl_fft_halfcomplex_wavetable_free", &mut [Arg::Handle { class: CLASS_FFT_HC_WAVETABLE, token: w as u64, release: true }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_real_workspace_alloc(n: usize) -> *mut c_void {
    call_handle("lind_gsl_fft_real_workspace_alloc", &mut [Arg::USize(n)], CLASS_FFT_REAL_WORKSPACE) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_real_workspace_free(w: *mut c_void) {
    let _ = call("lind_gsl_fft_real_workspace_free", &mut [Arg::Handle { class: CLASS_FFT_REAL_WORKSPACE, token: w as u64, release: true }]);
}

// ---- in-place transforms: data (n reals) + wavetable + shared workspace ------
macro_rules! fft_real_mixed {
    ($name:ident, $export:literal, $wtclass:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(data: *mut f64, stride: usize, n: usize, wt: *mut c_void, work: *mut c_void) -> c_int {
            if n == 0 {
                return 0;
            }
            let span = span_doubles(stride, n, 1); // real: 1 double per element
            let b = unsafe { core::slice::from_raw_parts_mut(data as *mut u8, span * 8) };
            call(
                $export,
                &mut [
                    Arg::InOut { dst: b, len: OutLen::Cap },
                    Arg::USize(stride),
                    Arg::USize(n),
                    Arg::Handle { class: $wtclass, token: wt as u64, release: false },
                    Arg::Handle { class: CLASS_FFT_REAL_WORKSPACE, token: work as u64, release: false },
                ],
            ) as c_int
        }
    };
}

fft_real_mixed!(gsl_fft_real_transform,        "lind_gsl_fft_real_transform",        CLASS_FFT_REAL_WAVETABLE);
fft_real_mixed!(gsl_fft_halfcomplex_transform, "lind_gsl_fft_halfcomplex_transform", CLASS_FFT_HC_WAVETABLE);
fft_real_mixed!(gsl_fft_halfcomplex_backward,  "lind_gsl_fft_halfcomplex_backward",  CLASS_FFT_HC_WAVETABLE);
fft_real_mixed!(gsl_fft_halfcomplex_inverse,   "lind_gsl_fft_halfcomplex_inverse",   CLASS_FFT_HC_WAVETABLE);

// ---- unpack: packed coefficients (n reals) -> full complex (2n doubles) ------
macro_rules! fft_unpack {
    ($name:ident, $export:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(coeff: *const f64, complex_out: *mut f64, stride: usize, n: usize) -> c_int {
            if n == 0 {
                return 0;
            }
            let in_span = span_doubles(stride, n, 1); // n packed reals
            let out_span = span_doubles(stride, n, 2); // 2n complex doubles
            let ib = unsafe { core::slice::from_raw_parts(coeff as *const u8, in_span * 8) };
            let ob = unsafe { core::slice::from_raw_parts_mut(complex_out as *mut u8, out_span * 8) };
            call(
                $export,
                &mut [Arg::Buf(ib), Arg::Out { dst: ob, len: OutLen::Cap }, Arg::USize(stride), Arg::USize(n)],
            ) as c_int
        }
    };
}

fft_unpack!(gsl_fft_real_unpack,        "lind_gsl_fft_real_unpack");
fft_unpack!(gsl_fft_halfcomplex_unpack, "lind_gsl_fft_halfcomplex_unpack");
