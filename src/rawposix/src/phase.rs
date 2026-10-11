//! Phase marks.
//!
//! A grate tells the runtime that the workload entered a phase with the lind
//! syscall `LIND_PHASE(n)`; the IMFS grate marks phase 1 when it has staged
//! its preloads and starts the program, which is where a build starts. An
//! embedder that counts what the workload asks of it registers a hook with
//! [`register`] to learn where each phase begins; TriSeal hands the mark to
//! its SGX shim, which records it in memory the host reads.

use std::sync::OnceLock;
use typemap::datatype_conversion::sc_convert_sysarg_to_usize;

/// Called with the phase number at every mark.
pub type PhaseHook = Box<dyn Fn(u64) + Send + Sync>;

static HOOK: OnceLock<PhaseHook> = OnceLock::new();

/// Call `hook` at every phase mark. Only the first hook registered is kept.
pub fn register(hook: PhaseHook) {
    let _ = HOOK.set(hook);
}

/// `LIND_PHASE(n)`: the workload entered phase `n`. Always succeeds; without
/// a hook the mark is ignored.
pub extern "C" fn phase_syscall(
    cageid: u64,
    phase_arg: u64,
    phase_cageid: u64,
    _arg2: u64,
    _arg2_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let phase = sc_convert_sysarg_to_usize(phase_arg, phase_cageid, cageid) as u64;
    if let Some(hook) = HOOK.get() {
        hook(phase);
    }
    0
}
