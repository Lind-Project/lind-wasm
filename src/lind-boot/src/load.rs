/// Wraps a raw Wasmtime `VMContext` pointer for cross-boundary use.
///
/// The `VmCtxWrapper` type provides an minimal wrapper around a non-null
/// pointer to a `VMContext`. It allows the pointer to be passed between
/// Wasmtime and 3i without exposing the raw pointer everywhere in the
/// codebase.
struct VmCtxWrapper {
    vmctx: NonNull<c_void>,
}

unsafe impl Send for VmCtxWrapper {}
unsafe impl Sync for VmCtxWrapper {}

/// Holds both the process identifier and the Wasmtime context needed
/// for cross-instance callbacks.
///
/// Each `WasmCallbackCtx` instance corresponds to one Cage or Grate
/// process (`pid`) and its runtime context (`VmCtxWrapper`).
#[repr(C)]
struct WasmCallbackCtx {
    pid: u64,
    vm: VmCtxWrapper,
}

/// The callback function registered with 3i uses a unified Wasm entry
/// function as the single re-entry point into the Wasm executable. It
/// receives an address inside grate that identifies the target handler.
/// When invoked, the callback calls the entry function inside the Wasm
/// module, passing this address as an argument. The entry function then
/// dispatches control to the corresponding per-syscall implementation
/// based on the address provided by `register_handler`.
/// To complete the bridge between host and guest, the system uses
/// `Caller::with()` to re-enter the  Wasmtime runtime context from the
/// host side.
///
/// This function is called by 3i when a syscall is routed to a grate.
pub extern "C" fn grate_callback_trampoline(
    ctx: *mut c_void,
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
    // Never unwind across the C boundary.
    let res = catch_unwind(AssertUnwindSafe(|| unsafe {
        // Validatation check
        if ctx.is_null() {
            return threei_const::GRATE_ERR;
        }

        // Convert back to WasmCallbackCtx
        let ctx = &*(ctx as *const WasmCallbackCtx);
        let opaque: *mut VMOpaqueContext = ctx.vm.vmctx.as_ptr() as *mut VMOpaqueContext;
        let vmctx_raw: *mut VMContext = VMContext::from_opaque(opaque);

        // Re-enter Wasmtime using the stored vmctx pointer
        Caller::with(vmctx_raw, |caller: Caller<'_, Host>| {
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
    }));

    match res {
        Ok(v) => v,
        Err(_) => threei_const::GRATE_ERR,
    }
}
