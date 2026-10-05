use crate::lind_wasmtime::host::{submit_grate_request, submit_grate_request_v2};
use crate::{cli::CliOptions, lind_wasmtime::host::HostCtx};
use threei::threei_const;
use wasmtime_lind_3i::v2_adapter::V2CallOutcome;
use wasmtime_lind_3i::*;
use wasmtime_lind_multi_process;

/// The callback function registered with 3i uses a unified Wasm entry
/// function as the single re-entry point into the Wasm executable.
///
/// This function receives an address inside grate that identifies the target handler.
/// When invoked, the callback submits the request to lower layer, passing this address
/// as an argument. The entry function then dispatches control to the corresponding
/// per-syscall implementation based on the address provided by `register_handler`.
pub extern "C" fn grate_callback_trampoline(
    in_grate_fn_ptr_u64: u64,
    cageid: u64,
    arg1: u64,
    arg1cageid: u64,
    arg2: u64,
    arg2cageid: u64,
    arg3: u64,
    arg3cageid: u64,
    arg4: u64,
    arg4cageid: u64,
    arg5: u64,
    arg5cageid: u64,
    arg6: u64,
    arg6cageid: u64,
) -> i64 {
    // Form the grate request with the provided arguments and the handler address
    let req = GrateRequest {
        handler_addr: in_grate_fn_ptr_u64,
        cageid: cageid,
        arg1,
        arg1cageid,
        arg2,
        arg2cageid,
        arg3,
        arg3cageid,
        arg4,
        arg4cageid,
        arg5,
        arg5cageid,
        arg6,
        arg6cageid,
    };

    // Submit the request to the host-side grate handler and return the result.
    match submit_grate_request(cageid, req) {
        Ok(ret) => ret,
        Err(_) => threei_const::GRATE_ERR,
    }
}

/// The V2 (variable-width) counterpart to `grate_callback_trampoline`,
/// registered as the Wasmtime runtime's `GrateTrampolineFnV2`
/// (`threei::register_trampoline_v2`). Carries a versioned host
/// pointer/length view (`args_ptr`/`argc`) instead of a fixed six-slot
/// tuple -- see `GrateTrampolineFnV2`'s own doc for the safety contract
/// this must uphold (copy `args_ptr[..argc]` into owned storage before
/// doing anything else, since the caller's storage is only valid for the
/// duration of this call).
pub extern "C" fn grate_callback_trampoline_v2(
    handler_id: u64,
    grateid: u64,
    caller_cage: u64,
    abi_version: u32,
    signature_id: u64,
    args_ptr: *const threei::V2RawArg,
    argc: u64,
    expects_result: u8,
    out_ty: *mut u8,
    out_bits: *mut u64,
) -> i32 {
    // Reject a caller-side portal built against a different transport ABI
    // version FIRST, before args_ptr[..argc] is ever interpreted: an ABI
    // mismatch means the wire shape of V2RawArg itself -- the very layout
    // from_raw_parts below would read -- may not agree between the caller
    // and this runtime, so nothing about args_ptr/argc can be trusted yet.
    if abi_version != threei::V2_ABI_VERSION {
        threei::set_last_v2_diagnostic(Some(format!(
            "V2 request abi_version {abi_version} does not match this runtime's {} -- \
             caller-side portal and runtime were built against incompatible transport versions",
            threei::V2_ABI_VERSION
        )));
        return threei::V2_OUTCOME_REJECTED;
    }

    let raw_args: Vec<threei::V2RawArg> = if argc == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(args_ptr, argc as usize) }.to_vec()
    };

    let Some(registration) = threei::get_v2_registration_by_id(handler_id) else {
        threei::set_last_v2_diagnostic(Some(format!(
            "no V2 registration found for handler id {handler_id}"
        )));
        return threei::V2_OUTCOME_REJECTED;
    };

    // Cheap pre-check only: signature_id is a compact hash the caller-side
    // portal captured from registration.signature.id() at install time. A
    // mismatch here is a fast, precise diagnostic ("caller resolved this
    // handler against a stale/different registration"), but a MATCH here
    // must never be treated as proof of a structural match on its own --
    // the authoritative check is V2AdapterCache::resolve's full V2Signature
    // comparison, reached via submit_grate_request_v2 below, which never
    // trusts this id alone (see V2Request::signature_id's own doc).
    if signature_id != registration.signature.id() {
        threei::set_last_v2_diagnostic(Some(format!(
            "V2 request signature_id {signature_id} does not match handler {handler_id}'s \
             registered signature id {} -- caller resolved this handler against a stale or \
             different registration",
            registration.signature.id()
        )));
        return threei::V2_OUTCOME_REJECTED;
    }

    let mut args: Vec<wasmtime::Val> = Vec::with_capacity(raw_args.len());
    for (i, a) in raw_args.iter().enumerate() {
        let val = match a.ty {
            0 => wasmtime::Val::I32(a.bits as i32),
            1 => wasmtime::Val::I64(a.bits as i64),
            2 => wasmtime::Val::F32(a.bits as u32),
            3 => wasmtime::Val::F64(a.bits),
            other => {
                // A well-formed caller-side portal only ever tags an arg with
                // one of the four types above (see linker.rs's V2 portal) --
                // an unrecognized tag means the request is corrupt or was
                // built against an incompatible layout. Reject it rather than
                // silently substituting a fabricated I32(0), which would feed
                // the adapter a value the caller never actually sent.
                threei::set_last_v2_diagnostic(Some(format!(
                    "V2 request arg {i} has unrecognized type tag {other} -- expected 0=I32, \
                     1=I64, 2=F32, or 3=F64"
                )));
                return threei::V2_OUTCOME_REJECTED;
            }
        };
        args.push(val);
    }

    let outcome = match submit_grate_request_v2(grateid, &registration, caller_cage, &args) {
        Ok(outcome) => outcome,
        Err(e) => V2CallOutcome::Rejected(e.to_string()),
    };

    match outcome {
        V2CallOutcome::Ok(results) => {
            if expects_result == 1 {
                let Some(result) = results.first() else {
                    threei::set_last_v2_diagnostic(Some(
                        "adapter returned no result but one was expected".to_string(),
                    ));
                    return threei::V2_OUTCOME_REJECTED;
                };
                let (ty, bits) = match result {
                    wasmtime::Val::I32(v) => (0u8, *v as u32 as u64),
                    wasmtime::Val::I64(v) => (1u8, *v as u64),
                    wasmtime::Val::F32(v) => (2u8, *v as u64),
                    wasmtime::Val::F64(v) => (3u8, *v),
                    _ => (0u8, 0u64),
                };
                unsafe {
                    *out_ty = ty;
                    *out_bits = bits;
                }
            }
            threei::V2_OUTCOME_OK
        }
        V2CallOutcome::Rejected(reason) => {
            threei::set_last_v2_diagnostic(Some(reason));
            threei::V2_OUTCOME_REJECTED
        }
        V2CallOutcome::Trapped(reason) => {
            threei::set_last_v2_diagnostic(Some(reason));
            threei::V2_OUTCOME_TRAPPED
        }
    }
}

/// Entry points for Wasmtime-backed multi-process syscalls.
///
/// These functions serve as the *host-side syscall entry stubs* for
/// Wasmtime-based multi-process support in Lind. They are exposed as
/// `extern "C"` function pointers and registered with 3i during the
/// initial runtime bootstrap in `execute_wasmtime()`.
///
/// At startup, `execute_wasmtime()` installs these function pointers into the
/// RawPOSIX handler table of the initial cage via `register_handler`.
/// This registration happens exactly once: during `fork()`, RawPOSIX
/// clones the parent cage’s handler table into the child, so all forked
/// processes automatically inherit these handlers. In contrast, `exec()`
/// replaces the guest program within an existing cage and therefore does
/// not require rebuilding or modifying the handler table in the lind runtime.
///
/// All syscalls in Lind first pass through RawPOSIX and 3i. For syscalls
/// such as `clone`, `exec`, and `exit`, RawPOSIX alone is insufficient,
/// because correct semantics require coordinated interaction with the
/// Wasmtime runtime (e.g., process creation, re-instantiation, or teardown
/// of execution state). These entry functions explicitly bridge that gap
/// by returning control from RawPOSIX/3i back into the Wasmtime-aware
/// multi-process implementation.
///
/// Each function is a thin forwarding stub that delegates the actual
/// syscall semantics to `wasmtime_lind_multi_process`, which performs
/// the required runtime-sensitive operations while preserving POSIX
/// behavior in a fully userspace implementation.
pub extern "C" fn clone_syscall_entry(
    cageid: u64,
    clone_arg: u64,
    clone_arg_cageid: u64,
    parent_cageid: u64,
    arg2_cageid: u64,
    child_cageid: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    wasmtime_lind_multi_process::clone_syscall::<HostCtx, CliOptions>(
        cageid,
        clone_arg,
        clone_arg_cageid,
        parent_cageid,
        arg2_cageid,
        child_cageid,
        arg3_cageid,
        arg4,
        arg4_cageid,
        arg5,
        arg5_cageid,
        arg6,
        arg6_cageid,
    )
}

pub extern "C" fn exec_syscall_entry(
    cageid: u64,
    path_arg: u64,
    path_arg_cageid: u64,
    argv: u64,
    argv_cageid: u64,
    envs: u64,
    envs_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    wasmtime_lind_multi_process::exec_syscall::<HostCtx, CliOptions>(
        cageid,
        path_arg,
        path_arg_cageid,
        argv,
        argv_cageid,
        envs,
        envs_cageid,
        arg4,
        arg4_cageid,
        arg5,
        arg5_cageid,
        arg6,
        arg6_cageid,
    )
}

pub extern "C" fn exit_syscall_entry(
    cageid: u64,
    exit_code: u64,
    exit_code_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    wasmtime_lind_multi_process::exit_syscall::<HostCtx, CliOptions>(
        cageid,
        exit_code,
        exit_code_cageid,
        arg2,
        arg2_cageid,
        arg3,
        arg3_cageid,
        arg4,
        arg4_cageid,
        arg5,
        arg5_cageid,
        arg6,
        arg6_cageid,
    )
}

#[cfg(test)]
mod v2_trampoline_validation_tests {
    use super::grate_callback_trampoline_v2;

    #[test]
    fn rejects_abi_mismatch_before_dispatch() {
        let raw = threei::V2RawArg {
            ty: 0,
            bits: 17,
            cageid: 1,
        };
        let mut out_ty = 0;
        let mut out_bits = 0;
        let outcome = grate_callback_trampoline_v2(
            0,
            0,
            0,
            threei::V2_ABI_VERSION.wrapping_add(1),
            0,
            &raw,
            1,
            0,
            &mut out_ty,
            &mut out_bits,
        );
        assert_eq!(outcome, threei::V2_OUTCOME_REJECTED);
    }

    #[test]
    fn rejects_unknown_argument_type_tag() {
        let registration = threei::V2Registration {
            grate_cage: 91_001,
            adapter_export: "__lind_v2_adapter_invalid_tag_test".to_string(),
            manifest_version: 1,
            signature: threei::V2Signature {
                params: vec![threei::V2ValueType::I32],
                results: vec![],
            },
            callback_params: vec![],
        };
        let signature_id = registration.signature.id();
        threei::register_lib_handler_v2_entry(
            91_002,
            "lib_invalid_tag_test",
            "invalid_tag_test",
            registration,
        );
        let (handler_id, _) =
            threei::get_lib_handler_v2(91_002, "lib_invalid_tag_test", "invalid_tag_test")
                .expect("registration must be visible");

        let raw = threei::V2RawArg {
            ty: 0xff,
            bits: 17,
            cageid: 91_002,
        };
        let mut out_ty = 0;
        let mut out_bits = 0;
        let outcome = grate_callback_trampoline_v2(
            handler_id,
            91_001,
            91_002,
            threei::V2_ABI_VERSION,
            signature_id,
            &raw,
            1,
            0,
            &mut out_ty,
            &mut out_bits,
        );
        assert_eq!(outcome, threei::V2_OUTCOME_REJECTED);

        threei::rm_cage_from_lib_handler_table_v2(91_002);
        threei::release_v2_registration_refs(91_002);
    }
}
