use crate::{cli::CliOptions, host::HostCtx};
use anyhow::anyhow;
use threei::threei_const;
use wasmtime::vm::{VMContext, VMOpaqueContext};
use wasmtime::{Caller, Instance};
use wasmtime_lind_3i::{VmCtxWrapper, get_vmctx, set_vmctx};
use wasmtime_lind_multi_process;

/// The callback function registered with 3i uses a unified Wasm entry
/// function as the single re-entry point into the Wasm executable.
///
/// When invoked, this function first uses the provided grateid to
/// retrieve the corresponding `VMContext` pointer from lind-3i’s global
/// runtime-state table. The `VMContext` identifies the Wasmtime store and
/// instance associated with the target grate and allows execution to
/// re-enter the correct runtime context.
///
/// This function receives an address inside grate that identifies the target handler.
/// When invoked, the callback calls the entry function inside the Wasm
/// module, passing this address as an argument. The entry function then
/// dispatches control to the corresponding per-syscall implementation
/// based on the address provided by `register_handler`.
///
/// To complete the bridge between host and guest, the system uses
/// `Caller::with()` to re-enter the  Wasmtime runtime context from the
/// host side.
///
/// This function is called by 3i when a syscall is routed to a grate.
///
/// todo: Currently this function is sent to 3i from [run::execute] function.
/// This will be updated to be sent from lind-boot in the future.
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
) -> i32 {
    let vmctx_wrapper: VmCtxWrapper = match get_vmctx(cageid) {
        Some(v) => v,
        None => {
            panic!("no VMContext found for cage_id {}", cageid);
        }
    };

    // Convert back to VMContext
    let opaque: *mut VMOpaqueContext = vmctx_wrapper.as_ptr() as *mut VMOpaqueContext;

    let vmctx_raw: *mut VMContext = unsafe { VMContext::from_opaque(opaque) };

    // Re-enter Wasmtime using the stored vmctx pointer
    let grate_ret = unsafe {
        Caller::with(vmctx_raw, |caller: Caller<'_, HostCtx>| {
            let Caller {
                mut store,
                caller: instance,
            } = caller;

            // Resolve the unified entry function once per call
            let entry_func = instance
                .host_state()
                .downcast_ref::<Instance>()
                .ok_or_else(|| anyhow!("bad host_state Instance"))?
                .get_export(&mut store, "pass_fptr_to_wt")
                .and_then(|f| f.into_func())
                .ok_or_else(|| anyhow!("missing export `pass_fptr_to_wt`"))?;

            let typed_func = entry_func.typed::<(
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
                u64,
            ), i32>(&mut store)?;

            // Call the entry function with all arguments and in grate function pointer
            typed_func.call(
                &mut store,
                (
                    in_grate_fn_ptr_u64,
                    cageid,
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
                ),
            )
        })
        .unwrap_or(threei_const::GRATE_ERR)
    };
    // Push the vmctx back to the global pool
    set_vmctx(cageid, vmctx_wrapper);
    grate_ret
}

/// Entry points for Wasmtime-backed multi-process syscalls.
///
/// These functions serve as the *host-side syscall entry stubs* for
/// Wasmtime-based multi-process support in Lind. They are exposed as
/// `extern "C"` function pointers and registered with 3i during the
/// initial runtime bootstrap in `execute()`.
///
/// At startup, `execute()` installs these function pointers into the
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
