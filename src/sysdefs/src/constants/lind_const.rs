// ===== Lind File System Root =====
pub const PATH_MAX: usize = 4096;
pub const LIND_ROOT: &str = "/home/lind/lind-wasm/src/RawPOSIX/tmp";

// ===== Lind specific ===== 
pub const FDKIND_KERNEL: u32 = 0;
pub const MAX_CAGEID: i32 = 1024; // Maximum cage ID allowed
pub const MAXFD: usize = 1024; // Maximum file descriptors per cage

/// Maximum linear memory size for a single Wasm module in the current lind-wasm runtime.
/// Since lind-wasm uses 32-bit memories, the linear memory address space is limited to 4 GiB.
/// This constant represents that theoretical upper bound (0xFFFF_FFFF bytes).
///
/// **This limit may be adjusted in the future if lind-wasm adopts 64-bit memories or other memory models.**
pub const MAX_LIND_SIZE: u64 = 0xFFFF_FFFF;
