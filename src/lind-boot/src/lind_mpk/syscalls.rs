
use std::env;
use std::mem::size_of;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;

use cage::{get_cage, lind_signal_init};
use crate::lind_mpk::RuntimeInfo::MPKRuntimeInfo;
use threei::threei_const;
use wasmtime_lind_multi_process::THREAD_START_ID;
use wasmtime_lind_utils::LindCageManager;

// Stored by execute.rs after it resolves __enable_syscall_interpose so that
// mpk_clone_syscall_entry can re-register a new handler in the child process.
pub static ENABLE_INTERPOSE_PTR: AtomicU64 = AtomicU64::new(0);

// Global LindCageManager shared with execute.rs. Set by init_mpk() before any
// cage is forked; accessed by mpk_clone_syscall_entry to increment the counter.
pub static LIND_MANAGER: OnceLock<Arc<LindCageManager>> = OnceLock::new();

// MPK has no Wasmtime epoch handler, so lind_signal_init receives a pointer to
// this static zero, matching the disable_signals behaviour used in wasmtime.
static MPK_EPOCH: AtomicU64 = AtomicU64::new(0);

// Function type matching the glibc __enable_syscall_interpose ABI.
type EnableInterposeF = unsafe extern "C" fn(
    handler: Option<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i32) -> i64>,
) -> c_int;

// ── Debug helpers ────────────────────────────────────────────────────────────

fn mpk_debug_enabled() -> bool {
    env::var_os("LIND_MPK_DEBUG").is_some()
}

fn mpk_debug(message: impl AsRef<str>) {
    if mpk_debug_enabled() {
        eprintln!("[lind-mpk] {}", message.as_ref());
    }
}

// ── Wire format for cross-socket syscall forwarding ──────────────────────────
// Both SyscallMsg and SyscallResp are POD; SOCK_SEQPACKET preserves boundaries.

#[repr(C)]
struct SyscallMsg {
    number: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
}

#[repr(C)]
struct SyscallResp {
    retval: i64,
}

// Per-process fd used by the child's syscall handler to reach the parent's
// worker thread.  Set once in the child immediately after fork; never mutated
// again in that process.
//
// Using a plain static instead of thread_local because after fork there is
// exactly one thread in the child and we never spawn any before we set this.
static CHILD_SOCKET_FD: AtomicI32 = AtomicI32::new(-1);

/// Syscall handler installed in the **child** process after fork.
///
/// Every syscall issued inside the child's dlmopen namespace is serialised over
/// the SOCK_SEQPACKET socket and dispatched by the parent's worker thread via
/// `threei::make_syscall`.  The child blocks on `recv` until the response
/// arrives, preserving synchronous POSIX semantics.
unsafe extern "C" fn child_syscall_handler(
    number: i64,
    a1: i64,
    a2: i64,
    a3: i64,
    a4: i64,
    a5: i64,
    a6: i64,
    _nargs: i32,
) -> i64 {
    let fd = CHILD_SOCKET_FD.load(Ordering::Acquire);
    assert!(fd >= 0, "[child_syscall_handler] socket fd not initialised");

    let msg = SyscallMsg {
        number: number as u64,
        a1: a1 as u64,
        a2: a2 as u64,
        a3: a3 as u64,
        a4: a4 as u64,
        a5: a5 as u64,
        a6: a6 as u64,
    };

    // Send the syscall request – blocking; no MSG_DONTWAIT.
    let sent = libc::send(
        fd,
        &msg as *const SyscallMsg as *const libc::c_void,
        size_of::<SyscallMsg>(),
        0,
    );
    assert!(
        sent as usize == size_of::<SyscallMsg>(),
        "[child_syscall_handler] send failed: {}",
        std::io::Error::last_os_error()
    );

    // Block until the parent's worker thread sends back the result.
    let mut resp = SyscallResp { retval: 0 };
    let recvd = libc::recv(
        fd,
        &mut resp as *mut SyscallResp as *mut libc::c_void,
        size_of::<SyscallResp>(),
        0, // blocking – no MSG_DONTWAIT
    );
    assert!(
        recvd as usize == size_of::<SyscallResp>(),
        "[child_syscall_handler] recv failed: {}",
        std::io::Error::last_os_error()
    );

    resp.retval
}

/// Called from the custom glibc inside the isolated dlmopen namespace when the
/// guest program invokes `clone`/`fork`.
///
/// Post-conditions (both parent and child return `child_cageid as i32`):
/// - A fresh cage ID is allocated and the parent's fdtable is copied for the child.
/// - The OS process is forked.
/// - **Parent**: a dedicated worker thread is spawned that blocks on `recv` and
///   dispatches every incoming `SyscallMsg` through `threei::make_syscall` on
///   behalf of the child cage, then sends the `SyscallResp` back.
/// - **Child**: `__enable_syscall_interpose` is re-registered with
///   `child_syscall_handler`, which forwards every syscall over the socket.
pub extern "C" fn mpk_clone_syscall_entry(
    cageid: u64, //This is rawposix' cage id as it performs make_syscall
    _clone_arg: u64,
    _clone_arg_cageid: u64,
    _parent_cageid: u64,
    _arg2_cageid: u64,
    _child_cageid_hint: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    // ── 1. Perform Runtime setup ───────────────────────────────
    let child_cageid = _child_cageid_hint; //Where does _child_cageid_hint come from?

    // Resolve the global LindCageManager (set by init_mpk before first execute).
    let lind_manager = LIND_MANAGER
        .get()
        .expect("mpk_clone: LIND_MANAGER not set – call init_mpk first");

    // MPK has no Wasmtime epoch; pass a pointer to a static zero so
    // lind_signal_init stores a valid (disabled) epoch handler address.
    let epoch_pointer: *mut u64 = MPK_EPOCH.as_ptr();

    // initialize the signal for the main thread of forked cage
    lind_signal_init(
        child_cageid,
        epoch_pointer,
        THREAD_START_ID,
        true, /* this is the main thread */
    );
    
    // // new cage created, increment the cage counter
    lind_manager.increment();
    
    // Notify threei of the cage runtime type
    threei::set_cage_runtime(child_cageid, threei_const::RUNTIME_TYPE_MPK);


    // ── 2. Create a SOCK_SEQPACKET socketpair for syscall forwarding ──────────
    //
    // parent_fd: owned by the parent's worker thread  (recv requests, send responses)
    // child_fd:  owned by the child process           (send requests, recv responses)
    let mut fds: [c_int; 2] = [-1, -1];
    let rc = unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET,
            0,
            fds.as_mut_ptr(),
        )
    };
    assert!(rc == 0, "mpk_clone: socketpair failed: {}", std::io::Error::last_os_error());
    let parent_fd = fds[0];
    let child_fd = fds[1];

    // ── 4. Fork the OS process ────────────────────────────────────────────────
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "mpk_clone: fork failed: {}", std::io::Error::last_os_error());

    if pid > 0 {
        // ════════════════════════════════════════════════════════════════════
        // PARENT PROCESS
        // ════════════════════════════════════════════════════════════════════

        // Parent does not need the child-side fd.
        unsafe { libc::close(child_fd) };

        // Update the child cage's RuntimeInfo with the child process PID.
        // The child cage was created earlier (before this fork handler was called),
        // so we need to update its runtime_info with the forked child's PID.
        if let Some(child_cage) = get_cage(child_cageid) {
            let parent_cage = get_cage(_parent_cageid).expect("parent cage not found");
            let parent_info = parent_cage.runtime_info.read();
            
            // Downcast to MPKRuntimeInfo to access the handles
            if let Some(parent_mpk) = parent_info.as_any().downcast_ref::<MPKRuntimeInfo>() {
                // Create new MPKRuntimeInfo for child with the child's PID
                let child_mpk_info = MPKRuntimeInfo::new(
                    parent_mpk.loader_cage_handle,
                    parent_mpk.loader_libc_handle,
                    parent_mpk.enable_interpose_fn,
                    pid, // child's OS process ID
                );
                *child_cage.runtime_info.write() = Box::new(child_mpk_info);
            }
        }
        else {
            panic!("mpk fork: no cage found with id {} (child_cageid)", child_cageid);
        }

        // Spawn a dedicated handler thread.  It blocks on recv (no polling)
        // and dispatches every syscall from the child cage through threei.
        thread::spawn(move || {
            loop {
                let mut msg = SyscallMsg {
                    number: 0,
                    a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, a6: 0,
                };

                // Block until the child sends a syscall request.
                let n = unsafe {
                    libc::recv(
                        parent_fd,
                        &mut msg as *mut SyscallMsg as *mut libc::c_void,
                        size_of::<SyscallMsg>(),
                        0, // blocking – no MSG_DONTWAIT
                    )
                };

                if n <= 0 {
                    // Child closed its socket end (process exited).  Worker exits.
                    break;
                }

                // Forward the syscall to threei on behalf of the child cage.
                let retval = threei::make_syscall(
                    child_cageid,  // self_cageid: acting as the child cage
                    msg.number,
                    0,             // _syscall_name: unused for native callers
                    child_cageid,  // target_cageid: child cage's own resources
                    msg.a1, child_cageid,
                    msg.a2, child_cageid,
                    msg.a3, child_cageid,
                    msg.a4, child_cageid,
                    msg.a5, child_cageid,
                    msg.a6, child_cageid,
                );

                let resp = SyscallResp { retval: retval as i64 };
                unsafe {
                    libc::send(
                        parent_fd,
                        &resp as *const SyscallResp as *const libc::c_void,
                        size_of::<SyscallResp>(),
                        0,
                    )
                };
            }

            unsafe { libc::close(parent_fd) };
        });

        // Return the child cage id to the parent.
        child_cageid as i32
    } else {
        // ════════════════════════════════════════════════════════════════════
        // CHILD PROCESS
        // ════════════════════════════════════════════════════════════════════

        // Child does not need the parent-side fd.
        unsafe { libc::close(parent_fd) };        // Publish the socket fd so child_syscall_handler can find it.
        CHILD_SOCKET_FD.store(child_fd, Ordering::Release);

        // Re-register the syscall interposition hook in the child so that all
        // subsequent syscalls from inside the dlmopen namespace are forwarded
        // through the socket to the parent's worker thread.
        let ptr = ENABLE_INTERPOSE_PTR.load(Ordering::Acquire);
        assert!(
            ptr != 0,
            "mpk_clone: ENABLE_INTERPOSE_PTR not set - call init_mpk first"
        );
        let enable_interpose: EnableInterposeF = unsafe { std::mem::transmute(ptr as usize) };
        let ret = unsafe { enable_interpose(Some(child_syscall_handler)) };
        assert!(ret == 0, "mpk_clone: __enable_syscall_interpose failed in child");

        // Return the child cage id in the child process too.
        0 as i32
    }
}

/// MPK exit handler called when a cage terminates.
///
/// This function handles cleanup for MPK-based cages:
/// - Closes the dlmopen handles for the isolated namespace
/// - If the cage's PID indicates a different process, kills that process
///
/// Called from shim_exit_handler based on the cage's runtime type.
pub extern "C" fn mpk_exit_syscall_entry(
    _cageid: u64,
    exit_status: u64,
    exiting_cageid: u64,
    _tid: u64,
    _arg2_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    mpk_debug(format!("mpk_exit: cage {} exiting with status {}", exiting_cageid, exit_status));

    // Get the exiting cage and retrieve its MPKRuntimeInfo
    if let Some(cage) = get_cage(exiting_cageid) {
        let runtime_info = cage.runtime_info.read();
        
        if let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() {
            let cage_pid = mpk_info.pid;
            let my_pid = unsafe { libc::getpid() };
            
            mpk_debug(format!("mpk_exit: cage_pid={}, my_pid={}", cage_pid, my_pid));
            
            // If the cage has a non-zero PID and it's different from our own,
            // kill that process (it's a forked child)
            if cage_pid != 0 {
                assert!(
                    cage_pid != my_pid,
                    "mpk_exit: Cannot kill self (cage_pid={}, my_pid={})",
                    cage_pid, my_pid
                );
                
                mpk_debug(format!("mpk_exit: killing child process {}", cage_pid));
                unsafe {
                    libc::kill(cage_pid, libc::SIGKILL);
                }
            }
            else { //This cage is running in the same process, so we need to clean up the linker state.
                // Close the dlmopen handles for this cage's isolated namespace
                mpk_debug("mpk_exit: closing dlmopen handles");
                unsafe {
                    libc::dlclose(mpk_info.loader_libc_handle);
                    libc::dlclose(mpk_info.loader_cage_handle);
                }
            }
            
        } else {
            mpk_debug(format!("mpk_exit: cage {} has no MPKRuntimeInfo", exiting_cageid));
        }
        
        let is_last = cage::signal::lind_thread_exit(exiting_cageid, THREAD_START_ID as u64);
        
        
        cage::cage_finalize(exiting_cageid);
    } else {
        mpk_debug(format!("mpk_exit: cage {} not found", exiting_cageid));
    }
    
    // Decrement the cage counter
    if let Some(lind_manager) = LIND_MANAGER.get() {
        lind_manager.decrement();
    }

    
    mpk_debug(format!("mpk_exit: cage {} cleanup complete", exiting_cageid));
    exit_status as i32
}
