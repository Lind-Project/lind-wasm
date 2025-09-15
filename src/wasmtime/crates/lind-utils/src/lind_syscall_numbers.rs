// Minimal set of syscall numbers used by Wasmtime side for Lind
// Keeps the runtime minimal and the rawposix dispatcher handles the rest
// Source of truth: Linux x86_64 syscall table
//   https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl
// (Historical overview: https://filippo.io/linux-syscall-table/)
// Keep these in sync with glibc's lind_syscall_num.h and RawPOSIX dispatcher
pub const MMAP_SYSCALL: i32 = 9;
pub const CLONE_SYSCALL: i32 = 56;
pub const FORK_SYSCALL: i32 = 57;
pub const EXEC_SYSCALL: i32 = 59;
pub const EXIT_SYSCALL: i32 = 60;

/// In secure mode, typemap performs two checks:
/// 1. Unused arguments must be null (0).
/// 2. `cageid`/`argcageid` must be `> 0` and `< MAX_CAGEID`.
///
/// Using 0 satisfies both rules: it naturally represents "unused"
/// and is guaranteed to be outside the valid cage ID range.
/// 
/// A special placeholder (e.g., `0xdeadbeef`) was considered, but would
/// require ensuring across all layers (including future microvisor work)
/// that the value can never collide with a legal encoding.
///
/// Placeholder for unused syscall arguments
pub const UNUSED_ARG: u64 = 0;
/// Placeholder for unused cage/grate IDs
pub const UNUSED_ID: u64 = 0;
/// Placeholder for unused syscall name
pub const UNUSED_NAME: u64 = 0;
