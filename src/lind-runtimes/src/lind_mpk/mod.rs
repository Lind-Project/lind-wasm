pub mod lind_if;
pub mod pkeys;

/// Virtual address space reserved for each MPK cage's memory.
pub const MPK_MEMORY_SIZE: usize = 4 * 1024 * 1024 * 1024;
/// Arena reserved directly after each cage's memory for forked children's stacks.
pub const GS_FORK_ARENA_SIZE: usize = 1 << 30;
/// Virtual address range reserved per cage: cage memory plus the arena.
pub const CAGE_RESERVATION_SIZE: usize = MPK_MEMORY_SIZE + GS_FORK_ARENA_SIZE;
