use crate::lind_wasmtime::host::submit_grate_request;
use threei::threei_const;
use wasmtime_lind_3i::*;

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
