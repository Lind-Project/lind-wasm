use crate::fs_calls::kernel_close;
use crate::sys_calls::exit_syscall;
use crate::syscall_table::*;
use cage::{add_cage, cagetable_clear, cagetable_init, timer::IntervalTimer, Cage, Vmmap};
use dashmap::DashMap;
use fdtables;
use parking_lot::{Mutex, RwLock};
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering::*};
use std::sync::Arc;
use sysdefs::constants::{
    EXIT_SUCCESS, FDKIND_KERNEL, LIND_ROOT, RAWPOSIX_CAGEID, STDERR_FILENO, STDIN_FILENO,
    STDOUT_FILENO, VERBOSE,
};
use threei::{register_handler, RUNTIME_TYPE_WASMTIME};

/// Function signature for a RawPOSIX syscall handler.
///
/// This is the low-level ABI used by the 3i dispatcher to invoke
/// RawPOSIX syscall implementations.
///
/// Semantics:
/// - `target_cageid` identifies the cage whose syscall is being executed.
/// - `argN` is the raw syscall argument value.
/// - `argN_cageid` indicates where `argN` should be interpreted from.
pub type RawCallFunc = extern "C" fn(
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

/// Register all RawPOSIX syscall handlers for a given cage.
///
/// This function walks the global `SYSCALL_TABLE` and registers each syscall
/// implementation with the 3i dispatcher. This function first converts each
/// RawPOSIX Rust function pointer into a raw `u64` address. Then, registers
/// the handler with runtime metadata (runtime type, e.g. Wasmtime, cageid).
///
/// Parameters:
/// - `self_cageid`: the cage for which syscalls are being registered.
///
/// Returns:
/// - 0 on success (mirrors underlying registration API).
/// - Panics on failure.
pub fn register_rawposix_syscall(self_cageid: u64) -> i32 {
    let mut ret = 0;
    // Walk through the syscall table
    for &(sysno, func) in SYSCALL_TABLE.iter() {
        // Convert to u64 func ptr
        let impl_fn_ptr = func as *const () as u64;
        // Register to handler table in 3i
        ret = register_handler(
            impl_fn_ptr,
            self_cageid, // current cageid
            sysno,
            RUNTIME_TYPE_WASMTIME, // runtime id
            1,                     // register
            RAWPOSIX_CAGEID,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        if ret != 0 {
            panic!(
                "register_rawposix_syscall: failed to register syscall {} handler, return code {}",
                sysno, ret
            );
        }
    }
    ret
}

/// Those functions are required by wasmtime to create the first cage. `verbosity` indicates whether
/// detailed error messages will be printed if set.
///
/// This function is called by the host runtime (e.g., Wasmtime) during startup
/// to bootstrap the RawPOSIX environment.
///
/// This function will do following things:
/// 1. Initialize global state (verbosity, cage table, virtual file descriptor tables).
/// 2. Register syscall handlers for the init cage.
/// 3. Ensure standard file descriptors (0, 1, 2) are always valid.
/// 4. Create and register the init cage (cageid 1 equivalent).
///
/// - The init cage is self-parented.
/// - STDIN/STDOUT/STDERR are force-initialized to avoid undefined behavior
///   in guest programs.
///
/// Parameters:
/// - `verbosity`: controls runtime logging verbosity.
pub fn rawposix_start(verbosity: isize) {
    let _ = VERBOSE.set(verbosity); //assigned to suppress unused result warning
                                    // init cage table
    cagetable_init();

    fdtables::register_close_handlers(FDKIND_KERNEL, fdtables::NULL_FUNC, kernel_close);

    // register syscalls for init cage
    register_rawposix_syscall(1);

    // Set up standard file descriptors for the init cage
    // TODO:
    // Replace the hardcoded values with variables (possibly by adding a LIND-specific constants file)
    let dev_null = CString::new(format!("{}/dev/null", LIND_ROOT)).unwrap();

    // Make sure that the standard file descriptors (stdin, stdout, stderr) are always valid
    // Standard input (fd = 0) is redirected to /dev/null
    // Standard output (fd = 1) is redirected to /dev/null
    // Standard error (fd = 2) is set to copy of stdout
    unsafe {
        libc::open(dev_null.as_ptr(), libc::O_RDONLY);
        libc::open(dev_null.as_ptr(), libc::O_WRONLY);
        libc::dup(1);
    }

    //init cage is its own parent
    let initcage = Cage {
        cageid: 1,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        parent: 1,
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(1),
        epoch_handler: DashMap::new(),
        signalhandler: DashMap::new(),
        pending_signals: RwLock::new(vec![]),
        sigset: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(Vmmap::new()),
    };

    // Add cage to cagetable
    add_cage(
        1, // cageid
        initcage,
    );

    // init fdtables for cageid 1
    fdtables::init_empty_cage(1);
    // Set the first 3 fd to STDIN / STDOUT / STDERR
    // STDIN
    fdtables::get_specific_virtual_fd(
        1,
        STDIN_FILENO as u64,
        FDKIND_KERNEL,
        STDIN_FILENO as u64,
        false,
        0,
    )
    .unwrap();
    // STDOUT
    fdtables::get_specific_virtual_fd(
        1,
        STDOUT_FILENO as u64,
        FDKIND_KERNEL,
        STDOUT_FILENO as u64,
        false,
        0,
    )
    .unwrap();
    // STDERR
    fdtables::get_specific_virtual_fd(
        1,
        STDERR_FILENO as u64,
        FDKIND_KERNEL,
        STDERR_FILENO as u64,
        false,
        0,
    )
    .unwrap();
}

/// Shut down the RawPOSIX runtime.
///
/// This function will check the global cage table and issue an `exit` syscall
/// for each remaining cage in the table.
///
/// Notes:
/// - The exit syscall in shutdown function is always issued on the main
/// thread (threadid = 1).
pub fn rawposix_shutdown() {
    let exitvec = cagetable_clear();

    for cageid in exitvec {
        exit_syscall(
            cageid as u64,       // target cageid
            EXIT_SUCCESS as u64, // status arg
            cageid as u64,       // status arg's cageid
            1,                   // always main thread
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
    }
}
