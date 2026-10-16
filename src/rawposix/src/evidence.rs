//! The run's evidence.
//!
//! The first cage of a run (the workload: in TriSeal, the in-toto grate that
//! records the build) hands the runtime what it recorded with the lind
//! syscall `LIND_EVIDENCE(request, len, out, capacity)`. An embedder that
//! signs for the run registers a hook with [`register`]; the hook gets the
//! request and returns what it signed, which is copied back to `out`.
//! TriSeal's signs, with the key its TEE attestation is bound to, a
//! statement that names the recorder's key and the last link of its chain,
//! which anchors that chain to the attestation.
//!
//! Only the first cage may make the call, and only once: lind's
//! `make-syscall` stamps the cage that really made it into the last
//! argument's cage id, so no other cage can claim to be the first, and no
//! cage can call again to have something else signed later.

use cage::memory::{check_addr_read, check_addr_rw};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::lind_platform_const::INIT_CAGEID;
use typemap::datatype_conversion::{sc_convert_buf, sc_convert_sysarg_to_usize};

/// Signs for the run: given the first cage's request, returns the bytes to
/// hand back to it, or a negative errno.
pub type EvidenceHook = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, i32> + Send + Sync>;

static HOOK: OnceLock<EvidenceHook> = OnceLock::new();

/// Whether the first cage has made its call.
static TAKEN: AtomicBool = AtomicBool::new(false);

/// Largest request accepted.
const MAX_REQUEST: usize = 64 << 20;

/// Handle `LIND_EVIDENCE` with `hook`. Only the first hook registered is
/// kept; without one the call fails with ENOSYS.
pub fn register(hook: EvidenceHook) {
    let _ = HOOK.set(hook);
}

/// `LIND_EVIDENCE(request, len, out, capacity)`: hand the runtime `len`
/// bytes at `request` and receive its answer in `out`. Returns the length of
/// the answer, or a negative errno: EPERM unless the caller is the first
/// cage and has not called before, ENOSYS without a hook, ENOSPC if `out` is
/// too small (the call is used up all the same).
pub extern "C" fn evidence_syscall(
    cageid: u64,
    req_arg: u64,
    req_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    out_arg: u64,
    out_cageid: u64,
    cap_arg: u64,
    cap_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    caller_cageid: u64,
) -> i32 {
    // `caller_cageid` is stamped by make-syscall with the cage that made the
    // call; `cageid` is whom the call is for, which a cage chooses.
    if caller_cageid != INIT_CAGEID || cageid != INIT_CAGEID {
        return syscall_error(Errno::EPERM, "evidence", "only the first cage may call");
    }
    let hook = match HOOK.get() {
        Some(hook) => hook,
        None => return syscall_error(Errno::ENOSYS, "evidence", "no evidence hook"),
    };

    let req = sc_convert_buf(req_arg, req_cageid, cageid);
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let out = sc_convert_buf(out_arg, out_cageid, cageid) as *mut u8;
    let capacity = sc_convert_sysarg_to_usize(cap_arg, cap_cageid, cageid);
    if len > MAX_REQUEST || req.is_null() || check_addr_read(cageid, req as u64, len).is_err() {
        return syscall_error(Errno::EFAULT, "evidence", "bad request buffer");
    }
    if out.is_null() || check_addr_rw(cageid, out as u64, capacity).is_err() {
        return syscall_error(Errno::EFAULT, "evidence", "bad answer buffer");
    }
    if TAKEN.swap(true, Ordering::SeqCst) {
        return syscall_error(Errno::EPERM, "evidence", "the evidence was handed in already");
    }

    let request = unsafe { std::slice::from_raw_parts(req, len) }.to_vec();
    let answer = match hook(&request) {
        Ok(answer) => answer,
        Err(e) => return e,
    };
    if answer.len() > capacity || answer.len() > i32::MAX as usize {
        return syscall_error(Errno::ENOSPC, "evidence", "answer buffer too small");
    }
    unsafe { std::ptr::copy_nonoverlapping(answer.as_ptr(), out, answer.len()) };
    answer.len() as i32
}
