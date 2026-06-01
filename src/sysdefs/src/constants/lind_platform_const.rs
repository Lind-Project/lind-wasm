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
pub const MAX_CAGEID: i32 = 2048;
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
/// Logical target Cage ID representing the **3i control layer itself**.
///
/// This constant is a *virtual target identifier* used to route calls
/// to 3i's own operations rather than to a concrete cage or the
/// RawPOSIX/Wasmtime layers.
///
/// Unlike `RAWPOSIX_CAGEID` and `WASMTIME_CAGEID`, which represent
/// execution backends, `THREEI_CAGEID` is intended for meta-level
/// operations that modify or mediate the 3i dispatch system.
///
/// ## Usage scenarios
/// - Used when interposing on 3i-specific management syscalls such as
///   `register_handler` and `copy_data_between_cages`.
/// - It will be registered as the `target_cageid` for these syscalls
///   during `lind-boot` initialization in RawPOSIX.
/// - At dispatch time, 3i interprets this value as a request to route
///   the call through its internal control-layer logic rather than
///   forwarding it to RawPOSIX or Wasmtime.
pub const THREEI_CAGEID: u64 = 999999;

/// Default stack size assigned to each cage
pub const DEFAULT_STACKSIZE: u32 = 8388608; // 8 MB
/// Size of guard pages
pub const GUARD_SIZE: u32 = 4096; // 4 KB

/// The starting index for function tables of wasm modules in Lind.
/// function index of 1 must be reserved for SIG_IGN constant for signal handling
/// so the function table starts from index 2.
pub const TABLE_START_INDEX: u32 = 2;

/// Cage ID for the initial (bootstrap) cage created during `rawposix_start`.
pub const INIT_CAGEID: u64 = 1;
/// Thread ID for the main thread of a cage.
pub const MAIN_THREADID: u64 = 1;
/// Number of instances to pre-allocate for the initial cage
pub const INSTANCE_NUMBER: usize = 5000;

/// Maximum execve recursion depth for shebang execution, 4 is the typical value used in Linux.
pub const MAX_SHEBANG_DEPTH: i32 = 4;

// Custom Dynamic loading Error Code for communication between host loader and guest
pub enum DylinkErrorCode {
    // dlopen errors
    EOPEN = 1,       // error opening the file
    ETYPE = 2,       // wrong file type (not wasm file)
    EDYLINKINFO = 3, // shared wasm module does not contain dylink section
    EDEPENDENCY = 4, // error when loading dependencies
    ESYMBOL = 5,     // undefined symbol

    // dlsym errors
    ENOHANDLE = 6, // invalid handle
    ENOFOUND = 7,  // symbol not found

    // dlclose errors
    ENOOPEN = 8, // closed library is not open

    // other errors
    EINTERNAL = 9, // other internal error occurs in dynamic loader
}

pub const FPCAST_FUNC_SIGNATURE: &str = "$fpcast_emu$";

/// Maximum number of grate workers that may exist for one grate handler.
///
/// This is a platform-wide configuration constant. lind-wasm preallocates
/// execution capacity for at most `MAX_GRATE_WORKERS` concurrent grate-call
/// workers, and the stack arena is sized accordingly.
///
/// Because this value is global rather than per-instance, every grate-enabled
/// instance reserves the same number of worker stack slots in linear memory.
pub const MAX_GRATE_WORKERS: usize = 32;

/// Size in bytes of the usable stack region assigned to one grate worker.
///
/// Each worker executes in its own `Store + Instance` context, but workers may
/// still attach to the same underlying linear memory. Therefore, every worker
/// must be given a disjoint stack slot inside the shared stack arena.
///
/// This constant specifies the usable portion of that per-worker slot.
pub const GRATE_STACK_SLOT_SIZE: u32 = 8 * 1024 * 1024;

/// Size in bytes of the guard region placed before each grate-worker stack slot.
///
/// The stack arena is laid out as repeated
///
/// `guard + usable stack slot`
///
/// segments, one per worker. The guard region exists to separate adjacent
/// worker stacks inside shared linear memory and to reduce the risk that stack
/// growth or stack corruption in one worker silently overlaps another worker’s
/// usable stack region.
pub const GRATE_STACK_GUARD_SIZE: u32 = 4 * 1024;

/// ------------------------------------------------------------------
use std::sync::{OnceLock, RwLock};

/// Global base address of the grate stack arena in linear memory.
///
/// The stack arena is reserved once during instance initialization and then
/// reused by grate-worker creation logic to derive each worker’s stack slot:
///
/// `worker_stack_base(i) = STACK_ARENA_BASE + (i - 1) * (guard + slot) + guard`
///
/// One additional reason this base is recorded explicitly is that although the
/// static worker-stack layout is retrieved from module metadata during instance
/// construction, the underlying stack size is still a compile-time parameter.
/// In other words, the arena geometry is not an immutable universal runtime
/// constant: different compiled modules may encode different stack sizing
/// choices.
///
/// This value is placed in `sysdefs` as a shared low-level platform constant
/// rather than being owned by `lind-3i`, `lind-multi-process`, or Wasmtime
/// directly. The main reason is dependency hygiene: all three components need
/// to agree on the same stack-arena base, but placing it in any one of those
/// layers would introduce circular dependency pressure between runtime,
/// interposition, and process-management code.
///
/// ------------------------------------------------------------------
///
/// Global per-cage storage for grate stack-arena base addresses.
///
/// The vector index is the cage ID. Each entry is `Some(base)` once that
/// cage's stack arena has been initialized, or `None` if it has not been
/// initialized yet.
///
/// [`OnceLock`] initializes the container only once, and [`RwLock`] provides
/// synchronized shared access to the per-cage entries.
pub static STACK_ARENA_BASES: OnceLock<RwLock<Vec<Option<u32>>>> = OnceLock::new();

/// Returns the global per-grate storage for stack-arena base addresses,
/// initializing the container on first use.
///
/// The outer [`OnceLock`] ensures that the storage itself is created only once,
/// while the inner [`RwLock<Vec<Option<u32>>>`] allows concurrent readers and
/// serialized updates as grates are initialized over time.
///
/// Each vector index corresponds to a grate ID. A value of `None` means that
/// the stack-arena base for that grate has not been initialized yet.
///
/// See [wasmtime/src/runtime/instance.rs] for the design and rationale
/// behind this constant and its initialization.
fn stack_arena_bases() -> &'static RwLock<Vec<Option<u32>>> {
    STACK_ARENA_BASES.get_or_init(|| RwLock::new(Vec::new()))
}

/// Records the stack-arena base address for `grate_id`.
///
/// The vector is grown as needed so that `grate_id` can be used directly as an
/// index. This function preserves one-time initialization semantics per grate:
/// if the given grate already has a recorded base, the function returns an
/// error instead of overwriting the existing value.
///
/// This is intended to be called once during instance initialization, after the
/// stack arena has been reserved and its resolved base address is known.
pub fn init_stack_arena_base(grate_id: usize, val: u32) -> Result<(), &'static str> {
    let mut bases = stack_arena_bases().write().unwrap();

    if bases.len() <= grate_id {
        bases.resize(grate_id + 1, None);
    }

    if bases[grate_id].is_some() {
        return Err("stack arena base already initialized for this grate");
    }

    bases[grate_id] = Some(val);
    Ok(())
}

/// Returns the recorded stack-arena base address for `cage_id`, if one exists.
///
/// `None` indicates that the grate has no initialized stack-arena base, either
/// because the grate has not been initialized yet or because the requested grate
/// ID is outside the current bounds of the global storage.
pub fn get_stack_arena_base(grate_id: usize) -> Option<u32> {
    let bases = stack_arena_bases().read().unwrap();
    bases.get(grate_id).copied().flatten()
}

/// Clears the recorded stack-arena base address for `grate_id`.
///
/// After this call, [`get_stack_arena_base`] will return `None` for the grate,
/// and [`init_stack_arena_base`] may be used again to record a new base.
///
/// Returns an error if the grate ID is out of bounds or if the entry was not
/// initialized.
pub fn unset_stack_arena_base(grate_id: usize) -> Result<(), &'static str> {
    let mut bases = stack_arena_bases().write().unwrap();

    let entry = bases
        .get_mut(grate_id)
        .ok_or("grate stack arena base storage not allocated for this grate")?;

    if entry.is_none() {
        return Err("stack arena base not initialized for this grate");
    }

    *entry = None;
    Ok(())
}

/// Copies the parent's recorded stack-arena base to a child cage during fork.
///
/// This preserves the same resolved stack-arena layout across the fork
/// boundary, rather than requiring the child to reconstruct it independently.
/// The operation fails if the parent has no initialized base or if the child
/// already has one recorded.
pub fn fork_stack_arena_base_for_child(
    parent_grate_id: usize,
    child_grate_id: usize,
) -> Result<(), &'static str> {
    let parent_base = get_stack_arena_base(parent_grate_id)
        .ok_or("parent grate stack arena base not initialized")?;
    init_stack_arena_base(child_grate_id, parent_base)
}
