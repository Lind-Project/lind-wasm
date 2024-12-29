// these are syscalls used in wasmtime
pub const MMAP_SYSCALL: i32 = 21;
pub const EXIT_SYSCALL: i32 = 30; 
pub const FORK_SYSCALL: i32 = 68;
pub const EXEC_SYSCALL: i32 = 69;