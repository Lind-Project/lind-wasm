use crate::{cli::CliOptions, lind_wasmtime::host::HostCtx};
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
