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

// Placeholder for unused syscall arguments
pub const NOTUSED_ARG: u64 = 0;
// Placeholder for unused cage/grate IDs
pub const NOTUSED_ID: u64 = 0;
// Placeholder for unused syscall name
pub const NOTUSED_NAME: u64 = 0;
