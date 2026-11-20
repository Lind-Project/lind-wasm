//! The MPK runtime: guest ELF programs loaded into memory-protection-key
//! isolated cages within the Lind process.

pub mod execute;
pub mod gs_fork;
pub mod lind_if;
pub mod loader;
pub mod pkeys;
#[allow(non_snake_case)]
pub mod RuntimeInfo;
pub mod signals;
pub mod syscalls;
pub mod trampoline;
mod host_rseq;

pub use execute::{execute_mpk, init_mpk, MpkLaunch};

/// Virtual address space reserved for each MPK cage's memory.
pub const MPK_MEMORY_SIZE: usize = 4 * 1024 * 1024 * 1024;
/// Arena reserved directly after each cage's memory for forked children's stacks.
pub const GS_FORK_ARENA_SIZE: usize = 1 << 30;
/// Virtual address range reserved per cage: cage memory plus the arena.
pub const CAGE_RESERVATION_SIZE: usize = MPK_MEMORY_SIZE + GS_FORK_ARENA_SIZE;
