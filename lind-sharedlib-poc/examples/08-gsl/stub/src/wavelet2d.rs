// Hand-written (Stage H). 2-D wavelet transforms — no callbacks, no new handle classes.
// Uses call/Arg/OutLen from lib.rs; CLASS_WAVELET/CLASS_WAVELET_WS from wavelet.rs (Stage F) and
// CLASS_MATRIX from the Tier-2 matrix module; c_int/c_void from core::ffi.
//
// Flat-array variants transform a size1 x size2 matrix stored with physical row stride `tda` in
// place (Arg::InOut, size1*tda doubles). Matrix variants take a gsl_matrix handle directly, so
// they are pure handles (wavelet + matrix + workspace).

// flat-array transform: (w, data, tda, size1, size2, work)
macro_rules! wavelet2d_flat {
    ($name:ident, $export:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(w: *mut c_void, data: *mut f64, tda: usize, size1: usize, size2: usize, work: *mut c_void) -> c_int {
            if size1 == 0 || tda == 0 {
                return 0;
            }
            let span = size1 * tda; // the matrix occupies size1 rows of `tda` doubles
            let b = unsafe { core::slice::from_raw_parts_mut(data as *mut u8, span * 8) };
            call(
                $export,
                &mut [
                    Arg::Handle { class: CLASS_WAVELET, token: w as u64, release: false },
                    Arg::InOut { dst: b, len: OutLen::Cap },
                    Arg::USize(tda),
                    Arg::USize(size1),
                    Arg::USize(size2),
                    Arg::Handle { class: CLASS_WAVELET_WS, token: work as u64, release: false },
                ],
            ) as c_int
        }
    };
}

// matrix-handle transform: (w, a, work) — all handles
macro_rules! wavelet2d_matrix {
    ($name:ident, $export:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(w: *mut c_void, a: *mut c_void, work: *mut c_void) -> c_int {
            call(
                $export,
                &mut [
                    Arg::Handle { class: CLASS_WAVELET, token: w as u64, release: false },
                    Arg::Handle { class: CLASS_MATRIX, token: a as u64, release: false },
                    Arg::Handle { class: CLASS_WAVELET_WS, token: work as u64, release: false },
                ],
            ) as c_int
        }
    };
}

wavelet2d_flat!(gsl_wavelet2d_transform_forward,   "lind_gsl_wavelet2d_transform_forward");
wavelet2d_flat!(gsl_wavelet2d_transform_inverse,   "lind_gsl_wavelet2d_transform_inverse");
wavelet2d_flat!(gsl_wavelet2d_nstransform_forward, "lind_gsl_wavelet2d_nstransform_forward");
wavelet2d_flat!(gsl_wavelet2d_nstransform_inverse, "lind_gsl_wavelet2d_nstransform_inverse");
wavelet2d_matrix!(gsl_wavelet2d_transform_matrix_forward,   "lind_gsl_wavelet2d_transform_matrix_forward");
wavelet2d_matrix!(gsl_wavelet2d_transform_matrix_inverse,   "lind_gsl_wavelet2d_transform_matrix_inverse");
wavelet2d_matrix!(gsl_wavelet2d_nstransform_matrix_forward, "lind_gsl_wavelet2d_nstransform_matrix_forward");
wavelet2d_matrix!(gsl_wavelet2d_nstransform_matrix_inverse, "lind_gsl_wavelet2d_nstransform_matrix_inverse");
