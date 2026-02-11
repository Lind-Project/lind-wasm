//! This file defines constants that are specific to the Lind-Wasm platform.
//!
/// ===== Lind File System Root =====
///
/// Maximum allowed path length in Lind.  
/// Used to validate path lengths during operations to prevent overflow.
pub const PATH_MAX: usize = 4096;

/// Root directory for lind filesystem used for chroot-based isolation.
pub const LINDFS_ROOT: &str = "/home/lind/lind-wasm/lindfs";

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
/// Logical target Cage ID representing RawPOSIX.
///
/// This constant is **not** a real cage ID. Instead, it is a  *semantic target
/// identifier* used by 3i to route calls to the RawPOSIX syscall implementation layer.
///
/// ## Usage scenarios
/// 1. During `lind-boot` initialization, syscalls that are expected to
///    go through the RawPOSIX layer (i.e., normal POSIX syscalls)
///    register their implementation functions into the 3i handler table
///    with `target_cageid = RAWPOSIX_CAGEID`.
/// 2. At dispatch time, 3i interprets this value as a request to invoke
///    the RawPOSIX syscall handler rather than a concrete cage instance.
pub const RAWPOSIX_CAGEID: u64 = 777777;
/// Logical target Cage ID representing **Wasmtime runtime entry points**.
///
/// This constant is a *virtual target identifier*, used to distinguish
/// calls that should be routed directly to Wasmtime-managed runtime
/// entry functions.
///
/// ## Usage scenarios
/// - Used during `lind-boot` initialization when registering Wasmtime
///   runtime entry functions (e.g., `fork`, `exec`, `exit`) into the 3i
///   handler table.
/// - When `target_cageid` is set to `WASMTIME_CAGEID`, 3i dispatches the
///   call to the corresponding Wasmtime entry function rather than
///   treating it as a RawPOSIX syscall or grate calls.
pub const WASMTIME_CAGEID: u64 = 888888;
