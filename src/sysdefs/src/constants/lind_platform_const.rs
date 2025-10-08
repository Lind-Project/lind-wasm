//! This file defines constants that are specific to the Lind-Wasm platform.
//!
/// ===== Lind File System Root =====
///
/// Maximum allowed path length in Lind.  
/// Used to validate path lengths during operations to prevent overflow.
pub const PATH_MAX: usize = 4096;
/// Root directory prefix used internally by Lind.  
/// This prefix is added before passing paths to the host system and removed
/// when returning paths to the user, ensuring the Lind filesystem remains
/// isolated from other parts. Only visible within the Lind codebase.
pub const LIND_ROOT: &str = "/home/lind/lind-wasm/src/RawPOSIX/tmp";

/// ===== Lind specific =====
///
/// Represents a virtual FD that has a mapping to a kernel file descriptor
/// in `fdtables`. Used to distinguish kernel-backed FDs from fully virtual ones
/// (e.g., in-memory pipes).
pub const FDKIND_KERNEL: u32 = 0;
/// Maximum allowed Cage ID.  
/// This limit is inherited from earlier implementations and may be
/// adjusted in the future.
pub const MAX_CAGEID: i32 = 1024;
pub const MAXFD: usize = 1024; // Maximum file descriptors per cage
/// Maximum linear memory size for a single Wasm module in the current lind-wasm runtime.
/// Since lind-wasm uses 32-bit memories, the linear memory address space is limited to 4 GiB.
/// This constant represents that theoretical upper bound (0xFFFF_FFFF bytes).
///
/// The implementation assumes that the allocated linear memory
/// region is contiguous.  
///
/// **This limit may be adjusted in the future if lind-wasm adopts 64-bit memories
/// or other memory models.**
pub const MAX_LINEAR_MEMORY_SIZE: u64 = 0xFFFF_FFFF;
/// Placeholder for unused syscall argument
pub const UNUSED_ARG: u64 = 0xDEADBEEF_DEADBEEF;
/// Placeholder for unused cage/grate ID
pub const UNUSED_ID: u64 = 0xCAFEBABE_CAFEBABE;
/// Placeholder for unused syscall name
pub const UNUSED_NAME: u64 = 0xFEEDFACE_FEEDFACE;
