//! Constants for threei 
//! 
/// todo: Need to replace by either linux number or purposed more appropriate error num
/// Special value for deregistration.  
/// When passed as `handlefunccage`, it removes all handler mappings for the given (targetcage, targetcallnum) pair.
pub const THREEI_DEREGISTER: u64 = 500;
/// (TODO: Intended to register a handler across all syscalls in the table for (targetcage, handlefunccage). ) 
pub const THREEI_MATCHALL: u64 = 501;
/// Generic error return code: API aborted.  
/// See function-level comments for specific usage details.  
pub const ELINDAPIABORTED: u64 = 0xFFFFFFFF;
/// ELINDESRCH: if either the source (targetcage) or destination (handlefunccage) is in the EXITING state.
/// See function-level comments for specific usage details.  
pub const ELINDESRCH: u64 = 0xFFFFFFFF;

/// This constant defines the maximum string length (`MAX_STRLEN`) used when copying strings
/// across cages, particularly in cases where the string length is not explicitly provided by the caller.
///
/// In such scenarios — for example, when copying a char* path from a Wasm program, the source may not
/// include the string length, so the system must scan for the null terminator manually. To prevent
/// runaway scans or buffer overflows, we impose an upper bound.
///
/// The value 4096 is chosen to match the typical Linux PATH_MAX, which defines the maximum length of
/// an absolute file path.
///
/// This constant is especially relevant when copytype == 1 (i.e., when performing a strncpy copy in
/// `copy_data_between_cages`).
pub const MAX_STRLEN: usize = 4096;
