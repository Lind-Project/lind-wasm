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
