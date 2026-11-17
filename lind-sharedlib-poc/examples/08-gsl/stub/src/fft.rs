// Hand-written (Stage C). FFT: in-place transforms + mixed-radix wavetable/workspace handles.
// Uses call/call_handle/Arg/OutLen from lib.rs; c_int/c_void from core::ffi.
//
// Radix-2 transforms are IN-PLACE over the caller's array -> Arg::InOut: the bytes are copied
// into the guest, transformed, and copied back into the same array. `gsl_complex_packed_array`
// is `double *` with 2n interleaved (re, im) values; real transforms use n values. Both honour
// `stride`, so the span the guest touches is ((n-1)*stride + 1) elements.

const CLASS_FFT_CX_WAVETABLE: u32 = 23;
const CLASS_FFT_CX_WORKSPACE: u32 = 24;

// span in doubles for a strided transform: `per` is 2 for complex (re,im), 1 for real.
fn span_doubles(stride: usize, n: usize, per: usize) -> usize {
    (n.saturating_sub(1) * stride + 1) * per
}

macro_rules! fft_inplace {
    ($name:ident, $export:literal, $per:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(data: *mut f64, stride: usize, n: usize) -> c_int {
            if n == 0 {
                return 0;
            }
            let span = span_doubles(stride, n, $per);
            let b = unsafe { core::slice::from_raw_parts_mut(data as *mut u8, span * 8) };
            call(
                $export,
                &mut [Arg::InOut { dst: b, len: OutLen::Cap }, Arg::USize(stride), Arg::USize(n)],
            ) as c_int
        }
    };
}

// ---- radix-2 (power-of-two n), no handles -----------------------------------
fft_inplace!(gsl_fft_complex_radix2_forward,  "lind_gsl_fft_complex_radix2_forward",  2);
fft_inplace!(gsl_fft_complex_radix2_backward, "lind_gsl_fft_complex_radix2_backward", 2);
fft_inplace!(gsl_fft_complex_radix2_inverse,  "lind_gsl_fft_complex_radix2_inverse",  2);
fft_inplace!(gsl_fft_real_radix2_transform,        "lind_gsl_fft_real_radix2_transform",        1);
fft_inplace!(gsl_fft_halfcomplex_radix2_inverse,   "lind_gsl_fft_halfcomplex_radix2_inverse",   1);
fft_inplace!(gsl_fft_halfcomplex_radix2_backward,  "lind_gsl_fft_halfcomplex_radix2_backward",  1);

#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_complex_radix2_transform(data: *mut f64, stride: usize, n: usize, sign: c_int) -> c_int {
    if n == 0 {
        return 0;
    }
    let span = span_doubles(stride, n, 2);
    let b = unsafe { core::slice::from_raw_parts_mut(data as *mut u8, span * 8) };
    call(
        "lind_gsl_fft_complex_radix2_transform",
        &mut [Arg::InOut { dst: b, len: OutLen::Cap }, Arg::USize(stride), Arg::USize(n), Arg::I32(sign)],
    ) as c_int
}

// ---- mixed-radix (arbitrary n): wavetable + workspace handles ---------------
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_complex_wavetable_alloc(n: usize) -> *mut c_void {
    call_handle("lind_gsl_fft_complex_wavetable_alloc", &mut [Arg::USize(n)], CLASS_FFT_CX_WAVETABLE) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_complex_wavetable_free(w: *mut c_void) {
    let _ = call("lind_gsl_fft_complex_wavetable_free", &mut [Arg::Handle { class: CLASS_FFT_CX_WAVETABLE, token: w as u64, release: true }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_complex_workspace_alloc(n: usize) -> *mut c_void {
    call_handle("lind_gsl_fft_complex_workspace_alloc", &mut [Arg::USize(n)], CLASS_FFT_CX_WORKSPACE) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_fft_complex_workspace_free(w: *mut c_void) {
    let _ = call("lind_gsl_fft_complex_workspace_free", &mut [Arg::Handle { class: CLASS_FFT_CX_WORKSPACE, token: w as u64, release: true }]);
}

macro_rules! fft_mixed {
    ($name:ident, $export:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(data: *mut f64, stride: usize, n: usize, wavetable: *mut c_void, work: *mut c_void) -> c_int {
            if n == 0 {
                return 0;
            }
            let span = span_doubles(stride, n, 2);
            let b = unsafe { core::slice::from_raw_parts_mut(data as *mut u8, span * 8) };
            call(
                $export,
                &mut [
                    Arg::InOut { dst: b, len: OutLen::Cap },
                    Arg::USize(stride),
                    Arg::USize(n),
                    Arg::Handle { class: CLASS_FFT_CX_WAVETABLE, token: wavetable as u64, release: false },
                    Arg::Handle { class: CLASS_FFT_CX_WORKSPACE, token: work as u64, release: false },
                ],
            ) as c_int
        }
    };
}

fft_mixed!(gsl_fft_complex_forward,  "lind_gsl_fft_complex_forward");
fft_mixed!(gsl_fft_complex_backward, "lind_gsl_fft_complex_backward");
fft_mixed!(gsl_fft_complex_inverse,  "lind_gsl_fft_complex_inverse");
