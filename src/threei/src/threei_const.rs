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
/// This value is used by Wasmtime to signal an error or invalid
/// state during Grate function dispatch (e.g., invalid pointer, missing
/// context, or lookup failure).
pub const GRATE_ERR: i32 = -0x1FFF_0003;
/// Runtime identifier for the Wasmtime-based execution environment.
/// This constant represents the runtime ID assigned to the Wasmtime runtime
/// when integrating with the 3i library. It is used to associate cages or
/// grates with Wasmtime as their execution backend and to select the corresponding
/// trampoline function when dispatching grate calls.
///
/// The value is expected to be globally unique among all runtimes registered with 3i
pub const RUNTIME_TYPE_WASMTIME: u64 = 1;
/// 3i-specific syscall number for `register_handler`.
///
/// Match the definition in `glibc/lind_syscall_num.h`.
/// TODO: When introducing a Rust-side unified syscall number table
/// (similar to glibc's `syscall_num` constants), move this constant there.
pub const REGISTER_HANDLER_SYSCALL: u64 = 1001;
/// 3i-specific syscall number for `copy_data_between_cages`.
///
/// Match the definition in `glibc/lind_syscall_num.h`.
/// TODO: When introducing a Rust-side unified syscall number table
/// (similar to glibc's `syscall_num` constants), move this constant there.
pub const COPY_DATA_BETWEEN_CAGES_SYSCALL: u64 = 1002;
/// 3i-specific syscall number for `copy_handler_table_to_cage`.
///
/// Match the definition in `glibc/lind_syscall_num.h`.
/// TODO: When introducing a Rust-side unified syscall number table
/// (similar to glibc's `syscall_num` constants), move this constant there.
pub const COPY_HANDLER_TABLE_TO_CAGE_SYSCALL: u64 = 1003;
/// 3i-specific syscall number for `register_lib_handler`.
///
/// Match the definition in `glibc/lind_syscall_num.h`.
pub const REGISTER_LIB_HANDLER_SYSCALL: u64 = 1004;
/// Base value for library-level fake syscall numbers.
///
/// Library-level 3i uses fake syscall numbers in the range
/// [LIBCALL_BASE, LIBCALL_BASE + N) to store per-symbol handlers in
/// HANDLERTABLE, reusing the existing syscall dispatch machinery.
/// These numbers must not overlap with real Linux syscall numbers or
/// the 3i meta-operation range (1001-1003).
pub const LIBCALL_BASE: u64 = 2000;
