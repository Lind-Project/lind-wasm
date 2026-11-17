// Hand-written (Stage G). Running / streaming statistics (gsl_rstat) — no callbacks.
// Uses call/call_f64/call_handle/Arg from lib.rs; c_int/c_void from core::ffi.
// A single opaque workspace handle: values are added one at a time, running moments read back.

const CLASS_RSTAT: u32 = 31;

#[unsafe(no_mangle)]
pub extern "C" fn gsl_rstat_alloc() -> *mut c_void {
    call_handle("lind_gsl_rstat_alloc", &mut [], CLASS_RSTAT) as *mut c_void
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_rstat_free(w: *mut c_void) {
    let _ = call("lind_gsl_rstat_free", &mut [Arg::Handle { class: CLASS_RSTAT, token: w as u64, release: true }]);
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_rstat_n(w: *mut c_void) -> usize {
    call("lind_gsl_rstat_n", &mut [Arg::Handle { class: CLASS_RSTAT, token: w as u64, release: false }]) as usize
}
// value FIRST, then the workspace (matches gsl_rstat_add's signature).
#[unsafe(no_mangle)]
pub extern "C" fn gsl_rstat_add(x: f64, w: *mut c_void) -> c_int {
    call("lind_gsl_rstat_add", &mut [Arg::F64(x), Arg::Handle { class: CLASS_RSTAT, token: w as u64, release: false }]) as c_int
}
#[unsafe(no_mangle)]
pub extern "C" fn gsl_rstat_reset(w: *mut c_void) -> c_int {
    call("lind_gsl_rstat_reset", &mut [Arg::Handle { class: CLASS_RSTAT, token: w as u64, release: false }]) as c_int
}

macro_rules! rstat_stat {
    ($name:ident, $export:literal) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(w: *mut c_void) -> f64 {
            call_f64($export, &mut [Arg::Handle { class: CLASS_RSTAT, token: w as u64, release: false }])
        }
    };
}

rstat_stat!(gsl_rstat_min,      "lind_gsl_rstat_min");
rstat_stat!(gsl_rstat_max,      "lind_gsl_rstat_max");
rstat_stat!(gsl_rstat_mean,     "lind_gsl_rstat_mean");
rstat_stat!(gsl_rstat_variance, "lind_gsl_rstat_variance");
rstat_stat!(gsl_rstat_sd,       "lind_gsl_rstat_sd");
rstat_stat!(gsl_rstat_rms,      "lind_gsl_rstat_rms");
rstat_stat!(gsl_rstat_skew,     "lind_gsl_rstat_skew");
rstat_stat!(gsl_rstat_kurtosis, "lind_gsl_rstat_kurtosis");
rstat_stat!(gsl_rstat_median,   "lind_gsl_rstat_median");
