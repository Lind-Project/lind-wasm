//! Threei (Three Interposition) module
use cage::memory::{check_addr, check_addr_read, check_addr_rw};
use core::panic;
use dashmap::DashSet;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use sysdefs::constants::lind_platform_const;
use sysdefs::constants::{PROT_READ, PROT_WRITE}; // Used in `copy_data_between_cages`
use typemap::datatype_conversion::sc_convert_uaddr_to_host;

use crate::handler_table::{
    _check_cage_handler_exist, _get_handler, _rm_cage_from_handler, _rm_grate_from_handler,
    copy_handler_table_to_cage_impl, register_handler_impl,
};
use crate::syscall_table::SYSCALL_TABLE;
use crate::threei_const;

pub const EXIT_SYSCALL: u64 = 60; // exit syscall number. Public for tests.

/// Function pointer type for rawposix syscall functions in SYSCALL_TABLE.
pub type RawCallFunc = fn(
    target_cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32;

///

/// Represents a callback function entry exported from a Wasm module into the host.
/// This will be only used in the Wasmtime side to store the function pointer and context
// and then passed into the 3i side for registration only when the `register_handler` call is made.
///
/// - `fn_ptr`:  Function pointer to the callback function inside the Wasm instance.
/// - `ctx_ptr`: Raw pointer to the Wasm module's `VMContext`.
///
/// It is never used or owned or deallocated by the 3i side, and the Wasmtime
/// runtime is responsible for managing its lifetime.
///
/// # `WasmGrateFnEntryPtr`
/// A transparent wrapper around a non-null pointer to a [`WasmGrateFnEntry`].
///
/// 1. The address referenced by this pointer is stable and will not be freed or moved
///    while it is stored in the global table.
/// 2. Accesses through this pointer are strictly read-only; no concurrent mutation occurs.
///
/// # Safety
/// [`WasmGrateFnEntry`] and [`WasmGrateFnEntryPtr`] are only used for reading access and 3i
/// will create new [`GrateFnEntry`] instances based on the data read from them.
/// 1) The object address pointed to by this pointer is stable and will not be released/moved
/// during its time in the table;
/// 2) We only perform read-only access (retrieve fields), and do not perform concurrent writes.
#[repr(C)]
pub struct WasmGrateFnEntry {
    pub fn_ptr: extern "C" fn(
        *mut c_void,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) -> i32,
    pub ctx_ptr: *mut c_void,
}

#[repr(transparent)]
#[derive(Clone)]
pub struct WasmGrateFnEntryPtr(NonNull<WasmGrateFnEntry>);

unsafe impl Send for WasmGrateFnEntryPtr {}
unsafe impl Sync for WasmGrateFnEntryPtr {}

impl WasmGrateFnEntryPtr {
    /// Creates a new wrapper from a raw pointer.
    ///
    /// Returns `None` if the pointer is null.
    #[inline]
    pub fn new(ptr: *const WasmGrateFnEntry) -> Option<Self> {
        NonNull::new(ptr as *mut WasmGrateFnEntry).map(WasmGrateFnEntryPtr)
    }

    /// Returns the raw pointer for FFI or read-only field access.
    #[inline]
    pub fn as_ptr(&self) -> *const WasmGrateFnEntry {
        self.0.as_ptr()
    }
}

/// Represents a single Grate function entry only used in the 3i side.
///
/// This struct is constructed by 3i **after** receiving a `WasmGrateFnEntryPtr`
/// from the Wasmtime side. 3i **does not** take ownership of Wasmtime-managed
/// memory; instead, it copies the raw fields (`fn_ptr`, `ctx_ptr`) into this
/// host-owned entry together with 3i’s own concurrency controls (`state`,
/// `call_lock`).
///
/// Each entry stores both:
/// - `fn_ptr`: a callback function pointer used to re-enter the Wasm module
///   from the host side (i.e., the actual logic Wasmtime executes when invoking
///   a Grate callback). See [`lind-3i`] and [`Wasmtime::Run`] crate for details.
/// - `ctx_ptr`: a pointer to the corresponding context information (`VMContext`) needed by
///    callback function.
/// - `state`: Lifecycle flag managed by 3i. Values: `ALIVE` -> `REVOKING` -> `DEAD`.
///   See `threei_const.rs` for more details.
/// - `call_lock`: Per-entry mutex used to **serialize re-entry** into the same
///   `(pid, tid)` `VMContext`. This prevents concurrent mutation of shared
///   Wasm-side state.
///
/// Since `VMContext` is per-thread, the global table `GLOBAL_GRATE` is keyed by `(pid, tid)`
/// to maintain per-Cage and per-thread associations.  
///
/// ## Safety model
/// 3i ensures memory-safety during teardown by following
/// “revoke-before-free” ordering:
/// 1) Remove the entry from `GLOBAL_GRATE` (no new handles),
/// 2) `state = REVOKING`,
/// 3) acquire `call_lock` (wait until the last in-flight call finishes),
/// 4) release Wasmtime-side resources (if 3i is responsible), then
/// 5) `state = DEAD`.
/// 6) Release `GrateFnEntry`.
///
/// Callers always:
/// - read `state` (must be `ALIVE`),
/// - lock `call_lock` (serialize re-entry),
/// - re-check `state` (guard against races while waiting for the lock),
/// - then invoke `fn_ptr(ctx_ptr, ...)`.
///
/// TODO: implement tid functionality. tid is set to zero for now as a placeholder.
///
/// This data structure is also accessed by the lind-3i crate within Wasmtime
/// to coordinate re-entry into the correct Wasm execution context.
#[repr(C)]
pub struct GrateFnEntry {
    pub fn_ptr: extern "C" fn(
        *mut c_void,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) -> i32,
    pub ctx_ptr: *mut c_void,
    pub state: AtomicU8,
    pub call_lock: Mutex<()>,
}

/// All re-entries for the same `(pid, tid)` are serialized by `call_lock`.
/// No two threads call `fn_ptr` concurrently with the same `ctx_ptr`.
/// Teardown follows revoke-before-free. Once `state = REVOKING`, no new calls are admitted
unsafe impl Send for GrateFnEntry {}
unsafe impl Sync for GrateFnEntry {}

/// Global mapping of `(pid, tid)` to `Arc<GrateFnEntry>`.
///
/// Used **ONLY** by 3i to look up the appropriate Grate callback and its associated `VMContext` when
/// performing a re-entry into Wasm.
pub static GLOBAL_GRATE: Lazy<RwLock<HashMap<(u64, u64), Arc<GrateFnEntry>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Registers a new `GrateFnEntry` into the global table `GLOBAL_GRATE`.
/// It takes a `grateid` (representing the Grate instance) and a u64 pointer (`entry_ptr_u64`) to a
/// `GrateFnEntry` that originates from the Wasmtime side.
///
/// The function validates that both the pointer and its contained `fn_ptr` and `ctx_ptr` are non-null
/// before inserting it into the table.
///
/// The key used is `(grateid, 0)`
/// TODO: the second component (`tid`) is reserved for future per-thread extensions.
///
/// Returns 0 on success, -1 on invalid pointer or null context.
fn _add_global_grate(grateid: u64, entry_ptr_u64: u64) -> i32 {
    if entry_ptr_u64 == 0 {
        return -1;
    }

    let src: &WasmGrateFnEntry = unsafe { &*(entry_ptr_u64 as usize as *const WasmGrateFnEntry) };
    if src.ctx_ptr.is_null() {
        return -1;
    }

    let entry = Arc::new(GrateFnEntry {
        fn_ptr: src.fn_ptr,
        ctx_ptr: src.ctx_ptr as *mut c_void,
        state: AtomicU8::new(threei_const::STATE_ALIVE),
        call_lock: Mutex::new(()),
    });

    let mut map = GLOBAL_GRATE.write().unwrap();
    if map.contains_key(&(grateid, 0)) {
        panic!(
            "[3i|_add_global_grate] attempt to overwrite existing grate entry for grateid: {}",
            grateid
        );
    }
    map.insert((grateid, 0), entry);

    0
}

/// Removes a `GrateFnEntry` associated with the given `grateid` from the global table.
/// TODO: the second component (`tid`) is reserved for future per-thread extensions.
fn _rm_from_global_grate(grateid: u64) {
    let mut map = GLOBAL_GRATE.write().expect("lock poisoned");
    map.remove(&(grateid, 0));
}

/// Invokes a stored Grate callback function (`fn_ptr`) corresponding to the given `grateid`.
/// The function first looks up `(grateid, 0)` in the global table to retrieve the associated
/// `GrateFnEntry`.
/// If found, it performs an unsafe call to the `fn_ptr`, passing in its `ctx_ptr` (the per-thread
/// `VMContext`) and argument pairs plus `self_cageid`.
/// This allows the 3i side to re-enter the Wasm runtime and execute a callback inside the Grate module.
///
/// Returns `Some(i32)` representing the grate call result, or `Some(-1)` if the entry does not exist.
fn _call_grate_func(
    grateid: u64,
    in_grate_fn_ptr_u64: u64,
    self_cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> Option<i32> {
    let entry = {
        let map = GLOBAL_GRATE.read().unwrap();
        map.get(&(grateid, 0)).cloned()
    }?;

    if entry.state.load(Ordering::Acquire) != threei_const::STATE_ALIVE {
        return Some(-1);
    }
    let _g = entry.call_lock.lock();

    if entry.state.load(Ordering::Acquire) != threei_const::STATE_ALIVE {
        return Some(-1);
    }

    let rc = (entry.fn_ptr)(
        entry.ctx_ptr,
        in_grate_fn_ptr_u64,
        self_cageid,
        arg1,
        arg1_cageid,
        arg2,
        arg2_cageid,
        arg3,
        arg3_cageid,
        arg4,
        arg4_cageid,
        arg5,
        arg5_cageid,
        arg6,
        arg6_cageid,
    );

    Some(rc)
}

/// EXITING_TABLE:
///
/// A grate/cage does not need to know the upper-level grate/cage information, but only needs
/// to manage where the call goes. I use a global variable table to represent the cage/grate
/// that is exiting. This table will be removed after the corresponding grate/cage performs
/// `exit_syscall`. During the execution of the corresponding operation, all other 3i calls
/// that want to operate the corresponding syscall will be blocked (additional check).
///
/// Only initialize once, and using dashset to support higher performance in high concurrency needs.
pub static EXITING_TABLE: Lazy<DashSet<u64>> = Lazy::new(|| DashSet::new());

/// This function registers an interposition rule, mapping a syscall number from a source cage to
/// a handler function in a destination grate or cage. Used for creating per-syscall routing rules
/// that enable one cage to interpose or handle syscalls on behalf of another.
///
/// For example:
/// I want cage 7 to have system call 34 call into my cage's function foo
///
/// Example:
/// register_handler(
///     foo_addr, 7,  34, SOME_ENTRY_PTR,
///    1, mycagenum,
///    ...)
///
///
/// If a conflicting mapping exists, the function panics to prevent accidental overwrite.
///
/// If a handler is already registered for this (syscall number, in grate function address) pair with the same
/// destination cage, the call is treated as a no-op.
///
/// ## Arguments:
/// - in_grate_fn_ptr_u64: Pointer to the function inside the grate that will handle this syscall.
/// - targetcage: The ID of the cage whose syscall table is being modified (i.e., the source of the syscall).
/// - targetcallnum: The syscall number to interpose on (can be treated as a match-all in some configurations).
/// - entry_ptr_u64: Pointer to the Grate function entry (contains `fn_ptr` and `ctx_ptr`).
/// - is_register: The operation flag to indicate whether to register or deregister.
/// - handlefunccage: The cage (typically a grate) that owns the destination function to be called.
///
/// ## Returns:
/// 0 on success.
/// ELINDESRCH if either the source (targetcage) or destination (handlefunccage) is in the EXITING state.
/// Panics if there is an attempt to overwrite an existing handler with a different destination cage.
pub fn register_handler(
    in_grate_fn_ptr_u64: u64,
    targetcage: u64,    // Cage to modify
    targetcallnum: u64, // Syscall number or match-all indicator. todo: Match-all.
    entry_ptr_u64: u64,
    is_register: u64,    // 0 for deregister
    handlefunccage: u64, // Grate cage id _or_ Deregister flag (`THREEI_DEREGISTER`) or additional information
    _arg3: u64,
    _arg3cage: u64,
    _arg4: u64,
    _arg4cage: u64,
    _arg5: u64,
    _arg5cage: u64,
    _arg6: u64,
    _arg6cage: u64,
) -> i32 {
    // Make sure that both the cage that registers the handler and the cage being registered are valid (not in exited state)
    if EXITING_TABLE.contains(&targetcage) || EXITING_TABLE.contains(&handlefunccage) {
        return threei_const::ELINDESRCH as i32;
    }

    // Add the `GrateFnEntry` to the global table
    _add_global_grate(handlefunccage, entry_ptr_u64);

    // Actual implementation is in handler_table module according to feature flag
    register_handler_impl(
        targetcage,
        targetcallnum,
        is_register,
        handlefunccage,
        in_grate_fn_ptr_u64,
    )
}

/// This copies the handler table used by a cage to another cage.  
/// This is often useful for calls like fork, so that a grate can later
/// add or remove entries.
///
/// Note that this call is itself made through a syscall and is thus
/// interposable.
///
/// ## Arguments:
/// - targetcage: The ID of the cage receiving the copied handler mappings.
/// - srccage: The ID of the cage whose handler mappings are being copied.
///
/// ## Returns:
/// - 0 on success.
/// - `ELINDESRCH` if either source or target cage is in the EXITING state.
/// - `ELINDAPIABORTED` if srccage has no existing handler table.
pub fn copy_handler_table_to_cage(
    _callnum: u64,
    targetcage: u64,
    srccage: u64,
    _arg1cage: u64,
    _arg2: u64,
    _arg2cage: u64,
    _arg3: u64,
    _arg3cage: u64,
    _arg4: u64,
    _arg4cage: u64,
    _arg5: u64,
    _arg5cage: u64,
    _arg6: u64,
    _arg6cage: u64,
) -> u64 {
    // Verifies that neither srccage nor targetcage are in the EXITING state to avoid
    // copying from or to a cage that may be invalid.
    if EXITING_TABLE.contains(&targetcage) || EXITING_TABLE.contains(&srccage) {
        return threei_const::ELINDESRCH as u64;
    }

    // Actual implementation is in handler_table module according to feature flag
    copy_handler_table_to_cage_impl(srccage, targetcage)
}

/// actually performs a call.  Not interposable
///
/// This actually performs a threei call.  It is not interposable.  This
/// is the most commonly used and simplest API, despite the number of
/// arguments.  All the code here does is route the call to the corresponding
/// handler and deal with error situations.
///
/// Note that this call is itself not interposable, since this is the base
/// call used to route other calls and do the interposition.  In theory, this
/// could be changed, but it doesn't seem useful to do so.
///
/// This is the main entry point used by cages or grates to invoke system calls through the
/// 3i layer. The function inspects the caller’s interposition configuration (if any) and
/// either routes the syscall to a grate for handling or directly invokes the corresponding
/// function in the RawPOSIX layer.
///
/// ## Behavior:
/// If the target_cageid is in the process of exiting and the syscall is not `EXIT_SYSCALL`,
/// the call is aborted early with `ELINDESRCH`
///
/// If the calling self_cageid has any handlers registered, the call is redirected to the
/// corresponding grate closure
///
/// If the syscall is EXIT_SYSCALL, performs global cleanup
///
/// If direct RawPOSIX call, falls back to invoking the syscall from SYSCALL_TABLE directly by number.
///
/// ## Arguments:
/// - self_cageid: The ID of the cage issuing the syscall (used to look up interposition rules
/// and access memory).
/// - syscall_num: Numeric syscall identifier, used to determine routing and rawposix function.
/// - syscall_name: A pointer (in Wasm memory) to the UTF-8 encoded string representing the
/// syscall name. Only relevant for grate calls.
/// - target_cageid: The target of the syscall (typically same as self_cageid, but may differ
/// in inter-cage calls).
/// - arg1..arg6: The six argument values passed to the syscall.
/// - arg1_cageid..arg6_cageid: The cage IDs that own the memory or context associated with
/// each argument.
///
/// Returns:
/// - `i32` syscall result.
/// - Returns `ELINDESRCH` if the target cage is in `EXITING_TABLE` and the syscall is not an exit.
/// - Returns `ELINDAPIABORTED` if the syscall number does not exist in the known syscall table.
/// - Returns the result of the interposed or rawposix syscall if executed successfully.
/// - Panics if the syscall was routed to a grate, but the corresponding exported function could
/// not be resolved.
pub fn make_syscall(
    self_cageid: u64, // is required to get the cage instance
    syscall_num: u64,
    _syscall_name: u64, // syscall name pointer in the calling Wasm instance
    target_cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Return error if the target cage/grate is exiting. We need to add this check beforehead, because make_syscall will also
    // contain cases that can directly redirect a syscall when self_cageid == target_id, which will bypass the handlertable check
    if EXITING_TABLE.contains(&target_cageid) && syscall_num != EXIT_SYSCALL {
        return threei_const::ELINDESRCH as i32;
    }

    // TODO:
    // if there's a better to handle
    // now if only one syscall in cage has been registered, then every call of that cage will check (extra overhead)
    if _check_cage_handler_exist(self_cageid) {
        if let Some((in_grate_fn_ptr_u64, grateid)) = _get_handler(self_cageid, syscall_num) {
            // <targetcage, targetcallnum, in_grate_fn_ptr_u64, this_grate_id>
            // Theoretically, the complexity is O(1), shouldn't affect performance a lot
            if let Some(ret) = _call_grate_func(
                grateid,
                in_grate_fn_ptr_u64,
                self_cageid,
                arg1,
                arg1_cageid,
                arg2,
                arg2_cageid,
                arg3,
                arg3_cageid,
                arg4,
                arg4_cageid,
                arg5,
                arg5_cageid,
                arg6,
                arg6_cageid,
            ) {
                return ret;
            } else {
                // syscall has been registered to register_handler but grate's entry function
                // doesn't provide
                // Panic here because this indicates error happens in wasmtime side when attaching
                // the module closure, which is a system-level error
                panic!(
                    "[3i|make_syscall] grate call not found! grateid: {}",
                    grateid
                );
            }
        }
    }

    // Cleanup two global tables for exit syscall
    if syscall_num == EXIT_SYSCALL {
        // todo: potential refinement here
        // since `_rm_grate_from_handler` searches all entries and remove desired entries..
        // to make things work as fast as possible, I use brute force here to perform cleanup
        _rm_grate_from_handler(self_cageid);
        // currently all cages/grates will store closures in global_grate table, so we need to
        // cleanup whatever its actually a cage/grate
        _rm_from_global_grate(self_cageid);
    }

    // Regular case (call from cage/grate to rawposix)
    if let Some(&(_, syscall_func)) = SYSCALL_TABLE.iter().find(|&&(num, _)| num == syscall_num) {
        let ret = syscall_func(
            target_cageid,
            arg1,
            arg1_cageid,
            arg2,
            arg2_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        );
        ret
    } else {
        eprintln!(
            "[3i|make_syscall] Syscall number {} not found!",
            syscall_num
        );
        threei_const::ELINDAPIABORTED as i32
    }
}

/***************************** trigger_harsh_cage_exit & harsh_cage_exit *****************************/
///
/// used to indicate a cage will terminate immediately.  Non-interposable
///
/// This is triggered by the caging or signaling infrastructure to indicate
/// that a cage will (uncleanly) exit.   This will trigger a harsh_cage_exit
/// call which will go through the respective grates until reaching threei's
/// version of the call.  This call can be thought of as notifying the grates
/// and microvisor of the harsh exit of the program.
///
/// This call is done first for two reasons.  First, this helps threei more
/// quickly block other calls which would go to that cage (if is a grate or
/// similar).   Second, if a grate does not pass the harsh_cage_exit call down,
/// it would not be cleaned up by threei.  This call gives threei a chance
/// to know that the cage is exiting and perform some cleanup.
///
/// This call is non-interposable, unlike harsh_cage_exit, which it calls.  
/// This is because this call is not a system call and cannot be triggered
/// by userspace (except performing some sort of action which causes the
/// system to be exited uncleanly by the caging software or similar).
///
/// ## Arguments:
/// - targetcage: The ID of the cage to be exited.
/// - exittype: Numeric reason code indicating why the cage is being forcibly exited (e.g., fault, violation, manual shutdown).
///
/// ## Returns:
/// None
pub fn trigger_harsh_cage_exit(targetcage: u64, exittype: u64) {
    // Mark this cage as exiting (block all future calls to it)
    EXITING_TABLE.insert(targetcage);

    // Eagerly remove references to this cage from handler table
    _rm_grate_from_handler(targetcage);

    // Attempt to call harsh_cage_exit on all grates that might interpose on it
    // Call harsh_cage_exit so that the interposable handler is triggered
    // This informs all relevant grates down the chain
    let _ = harsh_cage_exit(
        0, targetcage, // target to remove
        exittype,   // reason code
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    );
}

/// used to exit a cage due to memory fault or similar error.  Interposable
///
/// This enables threei to clean up information about a cage (or grate) which
/// has exited.  Namely, pending calls into that cage can have an error value
/// returned and new calls to that cage will result in an error.  A grate
/// receiving such a call must not assume that the calling cage exists anymore
/// in any table except a threei harsh_cage_exit call to a grate beneath it.
///
/// Note that this call may be interposed on but the memory of the cage which
/// is exiting *must not be referenced* (unlike with the normal exit syscall).
/// This is because this call may come from a cage protection fault or similar.
/// The cage which is apparently calling this, may not exist at the point the
/// call is received.
///
/// The microkernel / cage infrastructure uses this call itself when a cage
/// exits due to a fault.  A grate may make this call, but should prefer
/// exit, which allows other grates to cleanup while the cage state is intact.
///
/// ## Arguments:
/// - targetcage: The cage to be exited and cleaned up.
/// - exittype: The reason for the exit.
///
/// ## Returns:
/// - 0 on success.
///
/// TODO: could be extended to return error codes if cleanup or dispatch fails.
pub fn harsh_cage_exit(
    _callnum: u64,
    targetcage: u64,
    exittype: u64,
    _arg1cage: u64,
    _arg2: u64,
    _arg2cage: u64,
    _arg3: u64,
    _arg3cage: u64,
    _arg4: u64,
    _arg4cage: u64,
    _arg5: u64,
    _arg5cage: u64,
    _arg6: u64,
    _arg6cage: u64,
) -> u64 {
    // Call underlying exit syscall to perform cleanup
    // This is a direct underlying RawPOSIX call, so the `name` field will not be used.
    // We pass `0` here as a placeholder in the 3rd argument to avoid any unnecessary performance overhead.
    make_syscall(
        targetcage,
        EXIT_SYSCALL,
        0,
        targetcage,
        exittype,
        targetcage,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    // Actual implementation is in handler_table module according to feature flag
    _rm_cage_from_handler(targetcage);

    _rm_grate_from_handler(targetcage);

    // Remove from EXITING_TABLE if present (cleanup complete)
    EXITING_TABLE.remove(&targetcage);

    0 // success
}

/***************************** copy_data_between_cages *****************************/
///
/// CopyType represents the type of copy operation supported by copy_data_between_cages.
/// RawMemcpy: perform a raw memory copy of exactly `len` bytes.
/// Strncpy:   perform a string copy that stops at the first null byte or `len` limit.
#[repr(u64)]
enum CopyType {
    RawMemcpy = 0,
    Strncpy = 1,
}

/// Conversion implementation to map a numeric `u64` value into a `CopyType` enum.
/// Returns `Ok(CopyType)` for known values (0 = `RawMemcpy`, 1 = `Strncpy`).
/// Returns `Err(())` if the value does not match any supported variant.
impl TryFrom<u64> for CopyType {
    type Error = u64;
    fn try_from(v: u64) -> Result<Self, u64> {
        match v {
            0 => Ok(CopyType::RawMemcpy),
            1 => Ok(CopyType::Strncpy),
            _ => Err(v),
        }
    }
}

/// Helper function to validate that the requested length does not exceed a maximum.
/// Returns Ok(()) if the length is within bounds.
/// Returns Err(error_code) if the length is greater than the allowed maximum.
#[inline]
fn _validate_len(len: u64, max: u64) -> Result<(), u64> {
    if len > max {
        return Err(threei_const::ELINDAPIABORTED);
    }
    Ok(())
}

/// Helper function to validate that a given memory range is valid in a cage.
/// Uses the new vmmap helper functions to check range accessibility.
/// Returns Ok(()) if the range is valid and accessible.
/// Logs an error and returns Err(error_code) if the range is invalid.
#[inline]
fn _validate_range_read(cage: u64, addr: u64, len: usize, what: &str) -> Result<(), u64> {
    match check_addr_read(cage, addr, len) {
        Ok(_) => Ok(()),
        Err(_) => {
            eprintln!(
                "[3i|copy] range invalid: addr={:#x}, len={}, what={:?}",
                addr, len, what
            );
            Err(threei_const::ELINDAPIABORTED)
        }
    }
}

/// Helper function to validate that a given memory range has read/write access in a cage.
/// Uses the new vmmap helper functions to check range accessibility.
/// Returns Ok(()) if the range is valid and accessible with read/write permissions.
/// Logs an error and returns Err(error_code) if the range is invalid.
#[inline]
fn _validate_range_rw(cage: u64, addr: u64, len: usize, what: &str) -> Result<(), u64> {
    match check_addr_rw(cage, addr, len) {
        Ok(_) => Ok(()),
        Err(_) => {
            eprintln!(
                "[3i|copy] range invalid: addr={:#x}, len={}, what={:?}",
                addr, len, what
            );
            Err(threei_const::ELINDAPIABORTED)
        }
    }
}

/// Helper function to validate that a given memory range is valid in a cage.
/// Calls check_addr with the given cage, start address, length, and protection flags.
/// Returns Ok(()) if the range is valid and accessible.
/// Logs an error and returns Err(error_code) if the range is invalid.
///
/// Note: This function is kept for backward compatibility. Consider using
/// _validate_range_read or _validate_range_rw for better clarity.
#[inline]
fn _validate_range(cage: u64, addr: u64, len: usize, prot: i32, what: &str) -> Result<(), u64> {
    match check_addr(cage, addr, len, prot) {
        Ok(_) => Ok(()),
        Err(_) => {
            eprintln!(
                "[3i|copy] range invalid: addr={:#x}, len={}, what={:?}",
                addr, len, what
            );
            Err(threei_const::ELINDAPIABORTED)
        }
    }
}

/// A helper function that scans length for a null terminator in memory, mimicking
/// strlen behavior in C.
///
/// Given a raw pointer (src) to a memory region, this function checks for the first
/// null byte (0) within a specified `max_len`. This is used for safe copying of
/// C-style strings across cage boundaries.
///
/// ## Arguments:
/// - src: A raw pointer to the beginning of the string in the source cage's memory.
/// - max_len: The maximum number of bytes to scan, acting as a bound to prevent
/// overflow.
///
/// ## Returns:
/// - Some(length) if a null terminator is found within max_len.
/// - None if no null byte is found, indicating a malformed or unterminated string.
fn _strlen_in_cage(src: *const u8, max_len: usize) -> Option<usize> {
    unsafe {
        for i in 0..max_len {
            if *src.add(i) == 0 {
                return Some(i);
            }
        }
    }
    None // null terminator not found within max_len
}

/// copies memory across cages.  Interposable
///
/// This copies memory across cages.  One common use of this is to read
/// arguments which are passed by reference instead of by value.  The
/// source and destination cages may each be different from the calling
/// cage.  This may be useful for some grates.
///
/// Note that this call is itself interposable.  While threei does do some
/// checking, in theory, a grate may want to filter or disable this for
/// some descendant cages.
///
/// The maxsize and copytype arguments make the behavor act like strncpy or
/// memcpy.
///
/// ### Multithreading
/// This function performs *range and permission checks* and then copies bytes.
/// It does **not** acquire or hold any locks on the source or destination
/// mappings. In a multithreaded program, other threads (or cages) may
/// concurrently mutate or unmap these regions while the checks or the copy
/// are in progress. The behavior in that case is undefined from the caller’s
/// perspective (typical outcomes include torn reads/writes or faults).
///
/// ### Thread safety
/// This API is **not thread-safe w.r.t. the memory contents**. It is analogous
/// to calling `memcpy`/`strncpy` on raw pointers in C: the caller must ensure
/// that the specified intervals are exclusively owned or otherwise protected
/// for the entire duration of the call.
///
/// **Users need to ensure** that the specified memory regions remain valid,
/// mapped, and stable (i.e., not unmapped, re-mapped, or concurrently written)
/// for the entire duration of this operation.
///
/// ### Scope & constraints
/// - Cross-cage only: `srccage` and `destcage` must be different. Calls with
///   the same cage for source and destination are rejected with `ELINDAPIABORTED`.
/// - No shared memory is assumed between cages; overlapping regions across cages
///   are therefore impossible since wasm linear memory module.
/// - For intra-cage copies, callers should use a local memcpy/memmove path
///   instead of this 3i API.
///
/// ## Arguments:
/// - thiscage: ID of the cage initiating the call (used for address resolution).
/// - srcaddr: Virtual address in srccage where the data starts.
/// - srccage: Cage that owns the source data.
/// - destaddr: Destination virtual address in destcage; if 0, memory will be allocated
/// in this call.
/// - destcage: Cage that will receive the copied data.
/// - len: Number of bytes to copy for memcpy mode or maximum limit for strncpy.
/// - copytype: Type of copy: 0 = raw (memcpy), 1 = bounded string (strncpy).
///
/// ## Returns:
/// - `destaddr` (the destination address where data was written) on success.
/// - `ELINDAPIABORTED` on failure, due to:
///     - Invalid memory ranges or permission checks,
///     - Failed string validation (e.g., missing null terminator).
///     - Invalid copytype.
pub fn copy_data_between_cages(
    thiscage: u64,
    _targetcage: u64,
    srcaddr: u64,
    srccage: u64,
    destaddr: u64,
    destcage: u64,
    len: u64,
    _arg3cage: u64,
    copytype: u64, // 0=memcpy, 1=strncpy (bounded)
    _arg4cage: u64,
    _arg5: u64,
    _arg5cage: u64,
    _arg6: u64,
    _arg6cage: u64,
) -> u64 {
    // Disallow same-cage copies. This API is for cross-cage transfer only.
    if srccage == destcage {
        eprintln!(
            "[3i|copy] src and dest cage cannot be the same: {}",
            srccage
        );
        return threei_const::ELINDAPIABORTED;
    }

    // Reject requests where `len` exceeds the maximum allowed linear memory size
    // (`MAX_LIND_SIZE`), since such a copy would exceed the Wasm 32-bit address space.
    if let Err(code) = _validate_len(len, lind_platform_const::MAX_LINEAR_MEMORY_SIZE) {
        eprintln!("[3i|copy] length too large or zero: {}", len);
        return code;
    }
    // destaddr must be provided (no dynamic allocation support)
    if destaddr == 0 {
        panic!("Dynamic allocation not yet supported in copy_data_between_cages");
    }

    // Decide actual number of bytes to copy depending on CopyType
    // `memcpy`: Copies exactly n bytes from src to dest.
    // `strncpy`: Copies at most n bytes from src to dest.
    // If grate doesn't know the length of the content beforehand, it should use `strncpy` and set len to maximum
    // limits to avoid buffer overflow, so 3i needs to check the length of the content before copying.
    // Otherwise, grate should know the exact length of the content, for example the complex data structure etc.
    // In this case, it should use `memcpy` to copy the content.
    // So we have to check the address range and permissions accordingly before copying the data.
    let copy_len: usize = match CopyType::try_from(copytype) {
        // memcpy: just copy exactly len bytes
        Ok(CopyType::RawMemcpy) => len as usize,
        // strncpy: copy until '\0' or len limit, whichever comes first
        Ok(CopyType::Strncpy) => {
            // Validate that the source range is readable for at least `len` bytes
            if let Err(_e) = check_addr_read(srccage, srcaddr, len as usize) {
                eprintln!("[3i|copy] src precheck failed at start {:x}", srcaddr);
                return threei_const::ELINDAPIABORTED;
            }
            // Try to compute actual string length within limit
            let max_scan = len as usize;
            let host_src_try = srcaddr;
            if host_src_try == 0 {
                eprintln!("[3i|copy] host_src null");
                return threei_const::ELINDAPIABORTED;
            }
            let actual = match _strlen_in_cage(host_src_try as *const u8, max_scan) {
                Some(n) => n + 1,     // include '\0'
                None => len as usize, // assume max length
            };
            core::cmp::min(actual, len as usize)
        }
        // Reject invalid copytype values
        Err(other) => {
            eprintln!("[3i|copy] invalid copy type: {}", other);
            return threei_const::ELINDAPIABORTED;
        }
    };

    // Validate that src and dest ranges are accessible
    if let Err(code) = _validate_range_read(srccage, srcaddr, copy_len, "source") {
        return code;
    }
    if let Err(code) = _validate_range_rw(destcage, destaddr, copy_len, "destination") {
        return code;
    }

    // Translate user virtual addrs to host pointers
    let host_src_addr = srcaddr;
    let host_dest_addr = destaddr;
    if host_src_addr == 0 || host_dest_addr == 0 {
        // src addr or dest addr is null
        eprintln!("[3i|copy] host addr translate failed");
        return threei_const::ELINDAPIABORTED;
    }

    // Check for arithmetic overflow before doing pointer arithmetic
    if host_src_addr.checked_add(copy_len as u64).is_none()
        || host_dest_addr.checked_add(copy_len as u64).is_none()
    {
        eprintln!(
            "[3i|copy] address overflow: src={:#x} len={} dest={:#x}",
            srcaddr, copy_len, destaddr
        );
        return threei_const::ELINDAPIABORTED;
    }

    // Actually perform the copy
    unsafe {
        std::ptr::copy_nonoverlapping(
            host_src_addr as *const u8,
            host_dest_addr as *mut u8,
            copy_len,
        );
    }

    // Return destination address as success indicator
    destaddr
}
