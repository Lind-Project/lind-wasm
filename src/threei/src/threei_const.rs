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
/// Runtime identifier for the Wasmtime-based execution environment.
/// This constant represents the runtime ID assigned to the Wasmtime runtime
/// when integrating with the 3i library. It is used to associate cages or
/// grates with Wasmtime as their execution backend and to select the corresponding
/// trampoline function when dispatching grate calls.
///
/// The value is expected to be globally unique among all runtimes registered with 3i
pub const RUNTIME_WASMTIME: u64 = 1;
