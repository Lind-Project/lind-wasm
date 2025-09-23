//! Threei (Three Interposition) module
use cage::memory::check_addr;
use core::panic;
use dashmap::DashSet;
use once_cell::sync::Lazy;
use std::sync::Arc;
use nodit::interval::ie;
use sysdefs::constants::lind_platform_const;
use sysdefs::constants::{MAP_ANONYMOUS, MAP_PRIVATE, PROT_READ, PROT_WRITE, PROT_NONE, PROT_EXEC, PAGESHIFT, PAGESIZE}; // Used in `copy_data_between_cages`
use typemap::datatype_conversion::{sc_convert_buf, sc_convert_uaddr_to_host};
use cage::{get_cage, Cage, MemoryBackingType, VmmapEntry};

use crate::handler_table::{
    _check_cage_handler_exist, _get_handler, _rm_grate_from_handler,
    register_handler_impl, copy_handler_table_to_cage_impl,
    _rm_cage_from_handler
};
use crate::syscall_table::SYSCALL_TABLE;
use crate::threei_const;

pub const EXIT_SYSCALL: u64 = 60; // exit syscall number. Public for tests.
const MMAP_SYSCALL: u64 = 9; // mmap syscall number

/// Registers a closure into the `GLOBAL_GRATE` handler table for a specific grateid.
/// The closure is responsible for handling grate calls by dynamically looking up a Wasm-exported
/// function by name (following the `<call_name>_grate` suffix convention) and invoking it. This
/// function assumes that the `GLOBAL_GRATE` table is already initialized or initializes it if
/// needed. It panics if `grateid` exceeds the preallocated bounds (currently 1024 entries).
/// This function allows 3i to attach a per-grate function resolution mechanism, without needing
/// internal dispatch in the Wasm module.
///
/// ## Arguments:
/// - grateid: ID of the grate. Used as the index into `GLOBAL_GRATE`.
/// - callback: A boxed closure that takes the syscall name pointer and six argument pairs (value
/// and cage ID) and returns an i32. This closure handles dynamic lookup and execution.
///
/// ## Returns:
/// Always returns 0. Panics if grateid is out of bounds.
pub fn threei_wasm_func(
    grateid: u64,
    mut callback: Box<
        dyn FnMut(u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> i32
            + 'static,
    >,
) -> i32 {
    let index = grateid as usize;
    unsafe {
        if GLOBAL_GRATE.is_none() {
            _init_global_grate();
        }

        if let Some(ref mut vec) = GLOBAL_GRATE {
            if index < vec.len() {
                vec[index] = Some(callback);
            } else {
                panic!("[3i|threei_wasm_func] Index out of bounds: {}", index);
            }
        }
    }

    0
}

/// Function pointer type for rawposix syscall functions in SYSCALL_TABLE.
pub type RawCallFunc = fn(
    target_cageid: u64,
    arg1: u64, arg1_cageid: u64,
    arg2: u64, arg2_cageid: u64,
    arg3: u64, arg3_cageid: u64,
    arg4: u64, arg4_cageid: u64,
    arg5: u64, arg5_cageid: u64,
    arg6: u64, arg6_cageid: u64,
) -> i32;

/// Each entry in the `Vec` corresponds to a specific grate, and the index is used as its identifier
/// (`grate_id`). The position must be stable even if some grates are removed, so we use `Option` to
/// allow for "holes" in the vector.
///
/// - The outer `Option` represents whether the entire registry has been initialized.
/// - The inner `Vec<Option<Box<dyn FnMut(...)>>>` holds optional callback closures per grate.
/// - The inner `Option` allows a specific grate slot to be empty (e.g., after removal).
///
/// Each callback is a boxed closure that handles intercepted syscalls (registered at `threei_wasm_func`).
/// These callbacks are created dynamically and passed from the Wasmtime runtime.
/// Example layout:
///     GLOBAL_GRATE = Some(vec![
///         Some(Box::new(grate0_handler)),  // grate_id = 0
///         None,                            // grate_id = 1 (removed or not initialized)
///         Some(Box::new(grate2_handler)),  // grate_id = 2
///     ])
static mut GLOBAL_GRATE: Option<
    Vec<
        Option<
            Box<
                dyn FnMut(
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
            >,
        >,
    >,
> = None;

/// Initializes the `GLOBAL_GRATE` table if it is not already set.
///
/// Creates a fixed-size `Vec<Option<Closure>>` (currently 1024 entries) to store per-grate
/// closures, and sets `GLOBAL_GRATE` to a `Vec<Option<Closure>>` with a fixed number of
/// entries (currently 1024), each initially set to `None`.
///
/// This function should be called before any attempt to insert into or access `GLOBAL_GRATE`.
///
/// ## Arguments:
/// None
///
/// ## Returns:
/// None
fn _init_global_grate() {
    // Safety: Global mutable static variable GLOBAL_GRATE for mutable access
    unsafe {
        let vec = GLOBAL_GRATE
            .get_or_insert_with(|| Vec::with_capacity(lind_platform_const::MAX_CAGEID as usize));
        vec.resize_with(lind_platform_const::MAX_CAGEID as usize, || None);
    }
}

/// Marks the entry corresponding to a grateid in the `GLOBAL_GRATE` table as `None`,
/// unregistering its associated handler. This function is useful during cage teardown
/// or dynamic unloading of a grate.
///
/// ## Arguments:
/// - grateid: The index of the grate to be removed.
///
/// ## Returns:
/// None
fn _rm_from_global_grate(grateid: u64) {
    // Safety: Global mutable static variable GLOBAL_GRATE for mutable access
    unsafe {
        if let Some(ref mut global_grate) = GLOBAL_GRATE {
            if grateid < global_grate.len() as u64 {
                global_grate[grateid as usize] = None;
            }
        }
    }
}

/// Executes the registered handler closure for a given grateid, using the syscall name to
/// dynamically resolve the function to be called.
///
/// ## Arguments:
/// - grateid: ID of the target grate (used to locate the closure).
/// - call_name: Pointer to a UTF-8 encoded syscall name string in the calling Wasm instance.
/// - self_cageid: ID of the calling cage.
/// - arg1..arg6: Argument values to be passed to the syscall.
/// - arg1_cageid..arg6_cageid: Cage IDs corresponding to each argument
///
/// ## Returns:
/// `Some(i32)` if the call succeeds.
/// `None` if the handler or grate entry is missing.
fn _call_grate_func(
    grateid: u64,
    call_name: u64,
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
    // syscall_name from glibc is an address ptr inside wasm linear memory, so we need to
    // manually extract the string content from the address
    let call_ptr = sc_convert_buf(call_name, self_cageid, self_cageid);

    // Safety: Global mutable static variable GLOBAL_GRATE for mutable access
    unsafe {
        let vec = GLOBAL_GRATE.as_mut()?;
        let func = vec.get_mut(grateid as usize)?.as_mut()?;

        // The closure is then called with the extracted syscall name and the full set of
        // arguments + their corresponding cage IDs.
        Some(func(
            call_ptr as u64,
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
        ))
    }
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
///     NOTUSED, 7,  34, NOTUSED,
///    foo, mycagenum,
///    ...)
/// 
///
/// If a conflicting mapping exists, the function panics to prevent accidental overwrite.
///
/// If a handler is already registered for this (syscall number, function index) pair with the same
/// destination cage, the call is treated as a no-op.
///
/// ## Arguments:
/// - targetcage: The ID of the cage whose syscall table is being modified (i.e., the source of the syscall).
/// - targetcallnum: The syscall number to interpose on (can be treated as a match-all in some configurations).
/// - handlefunc: The function index (or exported function address) to register.
/// - handlefunccage: The cage (typically a grate) that owns the destination function to be called.
///
/// ## Returns:
/// 0 on success.
/// ELINDESRCH if either the source (targetcage) or destination (handlefunccage) is in the EXITING state.
/// Panics if there is an attempt to overwrite an existing handler with a different destination cage.
pub fn register_handler(
    _callnum: u64,
    targetcage: u64,    // Cage to modify
    targetcallnum: u64, // Syscall number or match-all indicator. Match-all: 1000.
    _arg1cage: u64,
    handlefunc: u64, // Function index to register (for grate, also called destination call) _or_ 0 for deregister
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

    // Actual implementation is in handler_table module according to feature flag
    register_handler_impl(
        targetcage,
        targetcallnum,
        handlefunc,
        handlefunccage,
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
    syscall_name: u64, // syscall name pointer in the calling Wasm instance
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
        if let Some((_call_index, grateid)) = _get_handler(self_cageid, syscall_num) {
            // <targetcage, targetcallnum, handlefunc_index_in_this_grate, this_grate_id>
            // Theoretically, the complexity is O(1), shouldn't affect performance a lot
            if let Some(ret) = _call_grate_func(
                grateid,
                syscall_name,
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
        return ret;
    } else {
        eprintln!(
            "[3i|make_syscall] Syscall number {} not found!",
            syscall_num
        );
        return threei_const::ELINDAPIABORTED as i32;
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
    Strncpy   = 1,
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
    if len > max { return Err(threei_const::ELINDAPIABORTED); }
    Ok(())
}

/// Helper function to validate that a given memory range is valid in a cage.
/// Calls check_addr with the given cage, start address, length, and protection flags.
/// Returns Ok(()) if the range is valid and accessible.
/// Logs an error and returns Err(error_code) if the range is invalid.
#[inline]
fn _validate_range(cage: u64, addr: u64, len: usize, prot: i32, what: &str) -> Result<(), u64> {
    match check_addr(cage, addr, len, prot) {
        Ok(_) => Ok(()),
        Err(_) => {
            eprintln!("[3i|copy] range invalid: addr={:#x}, len={}, what={:?}", addr, len, what);
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
        eprintln!("[3i|copy] src and dest cage cannot be the same: {}", srccage);
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
            if let Err(_e) = check_addr(srccage, srcaddr, len as usize, PROT_READ) {
                eprintln!("[3i|copy] src precheck failed at start {:x}", srcaddr);
                return threei_const::ELINDAPIABORTED;
            }
            // Try to compute actual string length within limit
            let max_scan = len as usize; 
            let host_src_try = sc_convert_uaddr_to_host(srcaddr, srccage, thiscage);
            if host_src_try == 0 {
                eprintln!("[3i|copy] host_src null");
                return threei_const::ELINDAPIABORTED;
            }
            let actual = match _strlen_in_cage(host_src_try as *const u8, max_scan) {
                Some(n) => n + 1, // include '\0'
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
    if let Err(code) = _validate_range(srccage, srcaddr, copy_len, PROT_READ, "source") {
        return code;
    }
    if let Err(code) = _validate_range(destcage, destaddr, copy_len, PROT_READ | PROT_WRITE, "destination") {
        return code;
    }
    
    // Translate user virtual addrs to host pointers
    let host_src_addr = sc_convert_uaddr_to_host(srcaddr, srccage, thiscage);
    let host_dest_addr = sc_convert_uaddr_to_host(destaddr, destcage, thiscage);
    if host_src_addr == 0 || host_dest_addr == 0 {
        // src addr or dest addr is null
        eprintln!("[3i|copy] host addr translate failed");
        return threei_const::ELINDAPIABORTED;
    }

    // Check for arithmetic overflow before doing pointer arithmetic
    if host_src_addr.checked_add(copy_len as u64).is_none()
        || host_dest_addr.checked_add(copy_len as u64).is_none()
    {
        eprintln!("[3i|copy] address overflow: src={:#x} len={} dest={:#x}", srcaddr, copy_len, destaddr);
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
