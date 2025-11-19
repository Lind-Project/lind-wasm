//! Constants for threei
//!
/// Special value for deregistration.  
/// When passed as `handlefunccage`, it removes all
/// handler mappings for the given (targetcage, targetcallnum) pair.
pub const THREEI_DEREGISTER: u64 = 500;
/// (TODO: Intended to register a handler across all syscalls
/// in the table for (targetcage, handlefunccage). )
pub const THREEI_MATCHALL: u64 = 501;
/// Generic error return code: API aborted.  
/// See function-level comments for specific usage details.  
pub const ELINDAPIABORTED: u64 = 0xE001_0001;
/// ELINDESRCH: if either the source (targetcage) or destination
// (handlefunccage) is in the EXITING state.
/// See function-level comments for specific usage details.  
pub const ELINDESRCH: u64 = 0xE001_0002;
/// Indicates a successful Grate call.
/// This value (0) is returned from functions that perform a Grate-side
/// operation or callback through Wasmtime when the call completes
/// normally without error.
pub const GRATE_OK: i32 = 0;
/// Indicates a failed Grate call.
/// This value (-1) is used by Wasmtime to signal an error or invalid
/// state during Grate function dispatch (e.g., invalid pointer, missing
/// context, or lookup failure).
pub const GRATE_ERR: i32 = -1;
/// Below are used to represent the states for Grate function entries:
/// When a grate function is registered, it's in `STATE_ALIVE`. 
/// When deregistering, it sets the state to `REVOKING` to reject new calls 
/// but let existing calls continue, once all calls are done, it sets it to 
/// `DEAD`.
/// 
/// Indicates that the `GrateFnEntry` is active and callable.
///
/// - New invocations through `_call_grate_func` are **allowed**.
/// - `ctx_ptr` and its associated Wasm `VMContext` are assumed to be valid.
/// - Normal re-entry into the Wasm module can proceed.
///
/// Used in the `state` field of `GrateFnEntry` of wasmtime 3i.
pub const STATE_ALIVE: u8 = 0;
/// Indicates that the entry is being revoked (teardown in progress).
///
/// - New invocations are **rejected** immediately.
/// - Existing in-flight calls may still be running.
/// - The removal path waits to acquire `call_lock` to ensure all in-flight
///   calls have completed before proceeding to full cleanup.
///
/// Used in the `state` field of `GrateFnEntry` of wasmtime 3i.
pub const STATE_REVOKING: u8 = 1;
/// Indicates that the entry has been fully invalidated and cleaned up.
///
/// - No invocations are allowed.
/// - The associated `ctx_ptr` and resources may have been released by
///   the Wasmtime side.
/// - Any further access to this entry is considered a logic error.
///
/// Used in the `state` field of `GrateFnEntry` of wasmtime 3i.
pub const STATE_DEAD: u8 = 2;
