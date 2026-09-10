
use std::env;
use std::mem::size_of;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Mutex, Condvar};
use std::thread;
use libc::{ 
    CLONE_CHILD_CLEARTID, CLONE_CHILD_SETTID, CLONE_CLEAR_SIGHAND, CLONE_DETACHED, CLONE_FILES, CLONE_FS, CLONE_INTO_CGROUP, CLONE_NEWCGROUP, CLONE_NEWIPC, CLONE_NEWNET, CLONE_NEWNS, CLONE_NEWPID, CLONE_NEWUSER, CLONE_NEWUTS, CLONE_PARENT, CLONE_PARENT_SETTID, CLONE_PIDFD, CLONE_PTRACE, CLONE_SETTLS, CLONE_SIGHAND, CLONE_SYSVSEM, CLONE_THREAD, CLONE_UNTRACED, CLONE_VFORK, CLONE_VM, EINVAL, PIDFD_GET_TIME_NAMESPACE
};
use cage::{get_cage, init_vmmap, lind_signal_init};
use crate::lind_mpk::RuntimeInfo::{
    MPKRuntimeInfo, MPKSupervisorCtxStack, MpkCageThreadInfo, MpkThreadInfo, LIND_MPK_MAX_CONTEXTS,
};
use crate::lind_mpk::execute::{setup_supervisor_stack, current_tid, setup_gs_context};
use threei::threei_const;
use wasmtime_lind_multi_process::THREAD_START_ID;
use wasmtime_lind_utils::LindCageManager;
use sysdefs::constants::syscall_const::{EXEC_SYSCALL, EXIT_GROUP_SYSCALL, EXIT_SYSCALL};
use sysdefs::logging::lind_debug_panic;
use sysdefs::data::sys_struct::CloneArgStruct;
use std::arch::asm;
use crate::lind_mpk::trampoline::{GS_SUPER_OS_TID};

// Stored by execute.rs after it resolves __enable_syscall_interpose so that
// mpk_clone_syscall_entry can re-register a new handler in the child process.
pub static ENABLE_INTERPOSE_PTR: AtomicU64 = AtomicU64::new(0);

// When set to true (via --no-interpose), __enable_syscall_interpose is resolved
// but never called, so all syscalls from inside the dlmopen namespace pass
// through to the kernel unintercepted.
pub static NO_INTERPOSE: AtomicBool = AtomicBool::new(false);

// Global LindCageManager shared with execute.rs. Set by init_mpk() before any
// cage is forked; accessed by mpk_clone_syscall_entry to increment the counter.
pub static LIND_MANAGER: OnceLock<Arc<LindCageManager>> = OnceLock::new();

// MPK has no Wasmtime epoch handler, so lind_signal_init receives a pointer to
// this static zero, matching the disable_signals behaviour used in wasmtime.
static MPK_EPOCH: AtomicU64 = AtomicU64::new(0);

// Function type matching the glibc __enable_syscall_interpose ABI.
use crate::lind_mpk::RuntimeInfo::EnableInterposeF;

// ── Debug helpers ────────────────────────────────────────────────────────────

fn mpk_debug_enabled() -> bool {
    env::var_os("LIND_MPK_DEBUG").is_some()
}

fn mpk_debug(message: impl AsRef<str>) {
    if mpk_debug_enabled() {
        eprintln!("[lind-mpk] {}", message.as_ref());
    }
}

/// replaces any occurrence of the old cage in the supervisor context stack with the new cage id.
/// This is used in the context of fork to return to a child cage
fn update_active_cage_context(cageid: u64, old_cageid: u64) {
    let gs_base: usize;
    let current_context_index: usize;
    unsafe {
        asm!(
            "mov %gs:0, {}",
            out(reg) current_context_index,
            options(nostack, preserves_flags, att_syntax)
        );
        asm!(
            "rdgsbase {}",
            out(reg) gs_base,
            options(nostack, preserves_flags, att_syntax)
        );
    }
    assert!(
        current_context_index < LIND_MPK_MAX_CONTEXTS,
        "mpk: current context index out of bounds: {}",
        current_context_index
    );
    let gs_data = gs_base as *mut MPKSupervisorCtxStack;
    unsafe {
        for context in &mut (*gs_data).contexts {
            if context.cage_id == old_cageid {
                context.cage_id = cageid;
            }
        }
    }
}

// ── Wire format for cross-socket syscall forwarding ──────────────────────────
// Both SyscallMsg and SyscallResp are POD; SOCK_SEQPACKET preserves boundaries.

#[repr(C)]
struct SyscallMsg {
    syscall_num: u64,
    syscall_name: u64,
    self_cageid: u64,
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
    a1: i64,
    a2: i64,
    a3: i64,
    a4: i64,
    a5: i64,
    a6: i64,
    _nargs: i32,
    number: i64,
    cage_id: u64,
) -> i64 {
    let fd = CHILD_SOCKET_FD.load(Ordering::Acquire);
    assert!(fd >= 0, "[child_syscall_handler] socket fd not initialised");

    let msg = SyscallMsg {
        syscall_num: number as u64,
        syscall_name: 0,
        self_cageid: cage_id,
        target_cageid: cage_id,
        arg1: a1 as u64,
        arg1_cageid: cage_id,
        arg2: a2 as u64,
        arg2_cageid: cage_id,
        arg3: a3 as u64,
        arg3_cageid: cage_id,
        arg4: a4 as u64,
        arg4_cageid: cage_id,
        arg5: a5 as u64,
        arg5_cageid: cage_id,
        arg6: a6 as u64,
        arg6_cageid: cage_id,
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

    //don't expect return value for exec and exit, just loop and wait for kill signal from parent
    if (number == EXIT_SYSCALL as i64) || (number == EXEC_SYSCALL as i64) || (number == EXIT_GROUP_SYSCALL as i64) {
       loop {
            std::thread::park();
        }
    }

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

/// Callback installed in the child process for glibc's make_threei_call.
///
/// The complete call is sent to the parent's worker so cage and pointer
/// translation metadata is preserved across the process boundary.
extern "C" fn child_make_threei_call_handler(
    self_cageid: u64,
    syscall_num: u64,
    syscall_name: u64,
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
) -> i64 {
    let fd = CHILD_SOCKET_FD.load(Ordering::Acquire);
    assert!(fd >= 0, "[child_make_threei_call_handler] socket fd not initialised");

    let msg = SyscallMsg {
        syscall_num,
        syscall_name,
        self_cageid,
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
    };
    unsafe {
        let sent = libc::send(
            fd,
            &msg as *const SyscallMsg as *const libc::c_void,
            size_of::<SyscallMsg>(),
            0,
        );
        assert!(
            sent as usize == size_of::<SyscallMsg>(),
            "[child_make_threei_call_handler] send failed: {}",
            std::io::Error::last_os_error()
        );
    
        let mut resp = SyscallResp { retval: 0 };
        let recvd = libc::recv(
            fd,
            &mut resp as *mut SyscallResp as *mut libc::c_void,
            size_of::<SyscallResp>(),
            0,
        );
        assert!(
            recvd as usize == size_of::<SyscallResp>(),
            "[child_make_threei_call_handler] recv failed: {}",
            std::io::Error::last_os_error()
        );
        
        resp.retval
    }

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
    let args = unsafe { &mut *(_clone_arg as *mut CloneArgStruct) };
    let isthread = args.flags & (CLONE_VM as u64) != 0; //CLONE_VM is set for threads, not for forked processes

    if (isthread) {
        //thread like semantics
        //Rawposix only respects CLONE_VM. Other flags are ignored.
        //Forwarded flags are
        //CLONE_SETTLS: We dont interfere with libc's TLS setup

   
        //Ignored/set flags are ()
        //CLONE_IO: The IO scheduler is out of scope for Lind.  
        //CLONE_FILES: not yet supported, needs to be implemented in Rawposix/fdtables
        //CLONE_FS: not yet supported, needs to be implemented in Rawposix/fdtables
        //CLONE_THREAD: always set for threads
        //CLONE_SIGHAND: not yet supported
        //CLONE_SYSVSEM: not yet supported
        args.flags |= ((CLONE_THREAD | CLONE_FILES | CLONE_FS | CLONE_SIGHAND | CLONE_SYSVSEM) as u64);

        //Ignored/cleared flags are (Most of these are simply out of scope as they are Linux specific, not POSIX)
        //CLONE_CHILD_CLEARTID: not supported
        //CLONE_CHILD_SETTID: not supported
        //CLONE_CLEAR_SIGHAND: not supported, could be implemented in Rawposix. PKRU is being reset on Linux signal delivery, so we need to provide a shim.
        //CLONE_DETACHED: not supported, historical
        //CLONE_INTO_CGROUP: not supported, 
        //CLONE_NEWCGROUP: not supported
        //CLONE_NEWIPC: not supported
        //CLONE_NEWNET: not supported
        //CLONE_NEWNS: not supported
        //CLONE_NEWPID: not supported
        //CLONE_NEWUSER: not supported
        //CLONE_NEWUTS: not supported
        //CLONE_PARENT: not supported
        //CLONE_PARENT_SETTID: not supported
        //CLONE_PIDFD: not supported
        //CLONE_PTRACE: not supported
        //CLONE_STOPPED: not supported
        //CLONE_UNTRACED: not supported
        //CLONE_VFORK: not supported
        args.flags &= !((CLONE_CHILD_CLEARTID | CLONE_CHILD_SETTID | CLONE_CLEAR_SIGHAND 
            | CLONE_DETACHED | CLONE_INTO_CGROUP | CLONE_NEWCGROUP | CLONE_NEWIPC | CLONE_NEWNET
            | CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWUSER | CLONE_NEWUTS | CLONE_PARENT
            | CLONE_PARENT_SETTID | CLONE_PIDFD | CLONE_PTRACE | CLONE_UNTRACED
            | CLONE_VFORK) as u64);
        args.child_tid = 0;
        args.parent_tid = 0;
        args.pidfd = 0;

        //still unsupported flags?
        if (args.flags & !((CLONE_VM | CLONE_SETTLS | CLONE_THREAD | CLONE_FILES | CLONE_FS | CLONE_SIGHAND | CLONE_SYSVSEM) as u64)) != 0 {
            lind_debug_panic(&format!("mpk_clone: unsupported clone flags: {:#x}", args.flags));
            return -EINVAL;
        }

        //do signal initialization for the new thread
        let cage = get_cage(_parent_cageid).expect("mpk_clone: cage not found");
        let rtInfo = cage.runtime_info.read();
        if let Some(mpk_info) = rtInfo.as_any().downcast_ref::<MPKRuntimeInfo>() {
            let next_tid = {
                let mut tid_lock = mpk_info.next_thread_id.write();
                let tid = *tid_lock;
                *tid_lock += 1;
                tid
            };
            lind_signal_init(
                _parent_cageid,
                MPK_EPOCH.as_ptr(),
                next_tid as i32,
                false, /* this is not the main thread */
            );

            // The child installs its own GS data after clone3 returns. The
            // supervisor stack must be created in the child because GS is
            // thread-local even though the address space is shared.
            drop(rtInfo);

            
            //here we launch the thread, in the parent process we return the thread id
            //TODO: make this launch 
            let _clone_result = unsafe {
                libc::syscall(libc::SYS_clone3, args as *const _, size_of::<CloneArgStruct>())
            };
    
            if (_clone_result < 0) {
                let err = std::io::Error::last_os_error();
                lind_debug_panic(&format!("mpk_clone: clone3 failed: {:#x}, errno: {}", _clone_result, err));
                return _clone_result as i32;
            }
            else if (_clone_result > 0) {
                //in the parent thread, we set the thread id in the cage's os_tid_map. 
                //TODO: The child thread should block until this is done. However, the os_tid_map is only used for killing sibling threads. 
                //If the child thread tries to exit before the parent thread sets the os_tid_map, it will still kill all other threads in the cage. 
                //This becomes only an issue if there are multiple uninitialized threads in the cage and one of them exits before all the other OS thread ids are set in the os_tid_map.
                cage.os_tid_map.insert(next_tid as i32, _clone_result as i64);
                
                return next_tid as i32;
            }
            else {
                let tid = current_tid();
                let thread_info = match setup_supervisor_stack(_parent_cageid, tid) {
                    Ok(info) => info,
                    Err(error) => {
                        lind_debug_panic(&format!(
                            "mpk_clone: setup_supervisor_stack failed for tid={}: {}",
                            tid, error
                        ));
                        return -EINVAL;
                    }
                };
                let mut cage_info = MpkCageThreadInfo {
                    thread_info: Arc::new(thread_info),
                    stack_addr: 0,
                    stack_base: 0,
                    stack_size: 0,
                };
                cage_info.thread_info.register_cage(_parent_cageid);
                if let Err(error) = cage_info.allocate_grate_stack() {
                    lind_debug_panic(&format!(
                        "mpk_clone: grate stack allocation failed for tid={}: {}",
                        tid, error
                    ));
                    return -EINVAL;
                }
                let runtime_info = cage.runtime_info.read();
                let mpk_info = runtime_info
                    .as_any()
                    .downcast_ref::<MPKRuntimeInfo>()
                    .expect("mpk_clone: cage runtime changed while creating thread");
                mpk_info.threads.write().insert(tid, cage_info);
                return 0;
            }
        }
        else {
            panic!("mpk_clone: cage {} has no MPKRuntimeInfo", _parent_cageid);
        }

    }
    else {
        //fork like semantics
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
        //FIXME: This associates the parent os_thread_id with the child cage. This can lead to an error on exit.
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
                    // Create new MPKRuntimeInfo for child with the child's PID.
                    // No initial_thread: per-thread resources (GsSegmentData, supervisor
                    // stack) live in the child's address space via fork's copy-on-write
                    // duplication and are not tracked by the parent.
                    let child_mpk_info = MPKRuntimeInfo::new(
                        parent_mpk.loader_cage_handle, //this is shared with the parent and needs to be treated carefully
                        parent_mpk.loader_libc_handle,
                        parent_mpk.enable_interpose_fn,
                        pid,
                        parent_mpk.memory_base,
                        parent_mpk.memory_size,
                        THREAD_START_ID + 1,
                        None,
                    );
                    // duplicate the child cage's vmmap so the parent can translate
                    // addresses when dispatching syscalls from the child via the socket.
                    if !parent_mpk.memory_base.is_null() {
                        let child_vmmap = parent_cage.vmmap.read().clone();
                        *child_cage.vmmap.write() = child_vmmap;
                    }
                    *child_cage.runtime_info.write() = Box::new(child_mpk_info);

                }
            }
            else {
                panic!("mpk fork: no cage found with id {} (child_cageid)", child_cageid);
            }

            // Spawn a dedicated handler thread.  It blocks on recv (no polling)
            // and dispatches every syscall from the child cage through threei.
            let parent_tid = current_tid();
            let parent_registered_cages = {
                let parent_cage = get_cage(_parent_cageid).expect("mpk_clone: parent cage not found");
                let runtime_info = parent_cage.runtime_info.read();
                runtime_info
                    .as_any()
                    .downcast_ref::<MPKRuntimeInfo>()
                    .and_then(|mpk_info| {
                        mpk_info
                            .threads
                            .read()
                            .get(&parent_tid)
                            .map(|thread| thread.thread_info.cage_ids.lock().unwrap().clone())
                    })
                    .unwrap_or_default()
            };

            thread::spawn(move || {
                //this tid is used to identify the os thread in the parent process. 
                //This is easier than using the child process's pid because the helper thread always outlives the child process.
                let helper_tid = current_tid(); 

                let (gs_ptr, cage_ptr, context_pages, context_pages_size) =
                    match setup_gs_context(child_cageid, helper_tid) {
                        Ok(result) => result,
                        Err(error) => {
                            eprintln!("[lind-mpk] parent worker thread: setup_gs_context failed for child cage {}: {}", child_cageid, error);
                            return;
                        }
                    };

                //create MPKThreadInfo and MPKCageThreadInfo and update in the child cage's MPK info
                
                // Create MpkThreadInfo from the GS context we just set up
                let thread_info = Arc::new(MpkThreadInfo {
                    gs_data: gs_ptr,
                    cage_data: cage_ptr,
                    context_pages,
                    context_pages_size,
                    supervisor_stack_base: 0, //not needed, the current thread was spawned on a supervisor stack
                    supervisor_stack_size: 0,
                    cage_ids: std::sync::Mutex::new(std::collections::HashSet::new()),
                });

                let mut registered_cage_ids = parent_registered_cages;
                registered_cage_ids.insert(child_cageid);

                for cage_id in &registered_cage_ids {
                    let mut cage_info = MpkCageThreadInfo {
                        thread_info: Arc::clone(&thread_info),
                        stack_addr: 0, //not needed, the worker thread does not enter the cage
                        stack_base: 0,
                        stack_size: 0,
                    };
                    cage_info.thread_info.register_cage(*cage_id);

                    if *cage_id != child_cageid {
                        //the helper thread never accesses the cage, so no stack needed.
                        cage_info.allocate_grate_stack();
                        mpk_debug(format!("allocated grate stack for cage {}", cage_id));
                    }

                    if let Some(cage) = get_cage(*cage_id) {
                        let runtime_info = cage.runtime_info.read();
                        if let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() {
                            mpk_info.threads.write().insert(helper_tid, cage_info);
                        } else {
                            eprintln!("[lind-mpk] parent worker thread: cage {} has no MPKRuntimeInfo", cage_id);
                            return;
                        }
                    } else {
                        eprintln!("[lind-mpk] parent worker thread: cage {} not found", cage_id);
                        return;
                    }
                }

                loop {
                    let mut msg = SyscallMsg {
                        syscall_num: 0,
                        syscall_name: 0,
                        self_cageid: 0,
                        target_cageid: 0,
                        arg1: 0, arg1_cageid: 0,
                        arg2: 0, arg2_cageid: 0,
                        arg3: 0, arg3_cageid: 0,
                        arg4: 0, arg4_cageid: 0,
                        arg5: 0, arg5_cageid: 0,
                        arg6: 0, arg6_cageid: 0,
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
                        child_cageid,
                        msg.syscall_num,
                        msg.syscall_name,
                        msg.target_cageid,
                        msg.arg1, msg.arg1_cageid,
                        msg.arg2, msg.arg2_cageid,
                        msg.arg3, msg.arg3_cageid,
                        msg.arg4, msg.arg4_cageid,
                        msg.arg5, msg.arg5_cageid,
                        msg.arg6, msg.arg6_cageid,
                    );

                    let resp = SyscallResp { retval };
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
            if !NO_INTERPOSE.load(Ordering::Acquire) {
                let ptr = ENABLE_INTERPOSE_PTR.load(Ordering::Acquire);
                assert!(
                    ptr != 0,
                    "mpk_clone: ENABLE_INTERPOSE_PTR not set - call init_mpk first"
                );
                let enable_interpose: EnableInterposeF = unsafe { std::mem::transmute(ptr as usize) };
                let ret = unsafe { enable_interpose(Some(child_syscall_handler), Some(child_make_threei_call_handler)) };
                assert!(ret == 0, "mpk_clone: __enable_syscall_interpose failed in child");
            }

            // Update the active context to reflect the new child cage id.
            update_active_cage_context(child_cageid, _parent_cageid);

            // Return the child cage id in the child process too.
            0 as i32
        }
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
            let mut cage_tid = 0;
            
            mpk_debug(format!("mpk_exit: cage_pid={}, my_pid={}", cage_pid, my_pid));
            
            // If the cage has a non-zero PID and it's different from our own,
            // kill that process (it's a forked child)
            if cage_pid != 0 {
                assert!(
                    cage_pid != my_pid,
                    "mpk_exit: Cannot kill self (cage_pid={}, my_pid={})",
                    cage_pid, my_pid
                );

                // Retrieve the thread ID from MPKSupervisorCtxStack and store it in cage_tid.
                unsafe {
                    asm!(
                        "mov %gs:{gs_super_os_tid_offset}, {cage_tid}",
                        cage_tid = out(reg) cage_tid,
                        gs_super_os_tid_offset = const GS_SUPER_OS_TID,
                        options(att_syntax)
                    );
                }
                
                
                mpk_debug(format!("mpk_exit: killing child process {}", cage_pid));
                unsafe {
                    libc::kill(cage_pid, libc::SIGKILL);
                }
            }
            else { //This cage is running in the same process, so we need to clean up the linker state.
                // Remove the exiting thread's MpkThreadInfo from the map; Drop frees its
                // supervisor stack and GsSegmentData.
                cage_tid = current_tid();
                
                
                // Close the dlmopen handles for this cage's isolated namespace
                mpk_debug("mpk_exit: closing dlmopen handles");
                unsafe {
                    libc::dlclose(mpk_info.loader_libc_handle);
                    libc::dlclose(mpk_info.loader_cage_handle);
                }
                // Unmap the cage's 4 GB virtual address space.
                if !mpk_info.memory_base.is_null() && mpk_info.memory_size > 0 {
                    mpk_debug("mpk_exit: unmapping cage memory region");
                    unsafe { libc::munmap(mpk_info.memory_base, mpk_info.memory_size); }
                }
                // Any remaining MpkThreadInfos in the threads map (e.g. for threads that
                // exit without going through mpk_exit) are freed when MPKRuntimeInfo
                // is dropped (cage_finalize below).
            }
            
            let thread_info = if let Some(cage_thread_info) = mpk_info.threads.write().remove(&cage_tid) {
                mpk_debug(format!("mpk_exit: freed supervisor stack for tid={}", cage_tid));
                Some(Arc::clone(&cage_thread_info.thread_info))
            } else {
                None
            };

            if let Some(thread_info) = thread_info {
                let cage_ids = thread_info.cage_ids.lock().unwrap().clone();
                for cage_id in cage_ids {
                    if cage_id == exiting_cageid {
                        continue;
                    }
                    if let Some(cage) = get_cage(cage_id) {
                        let runtime_info = cage.runtime_info.read();
                        if let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() {
                            mpk_info.threads.write().remove(&cage_tid);
                            //log the removal of the thread info for this cage
                            mpk_debug(format!("mpk_exit: removed thread info for tid={} from cage {}", cage_tid, cage_id));
                        }
                    }
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

    //Here the thread is terminated. (exit does not return in this implementation)
    //it is assumed that there are no references to rust objects stored on the supervisor stack at this point.
    unsafe {
        libc::syscall(libc::SYS_exit, 0);
    }
    
    0 //dummy
}



pub extern "C" fn mpk_set_return_stack_syscall_entry(
    _cageid: u64,
    stack_ptr: u64,
    _stack_ptr_cageid: u64,
    _arg2: u64,
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
    //here we are in the supervisor
    //the MPK context stack looks like this
    // |--------------------|
    // | ......             |
    // |--------------------|
    // | context n-1        |
    // |--------------------|
    // | context n          |
    // |--------------------|

    // context n is the context (rsp, pkru) of the grate that called us. 
    // => we need to modify rsp in context n - 1

    //assembly read gsbase
    let gsbase: u64;
    unsafe {
        asm!(
            "rdgsbase {0}",
            out(reg) gsbase,
            options(nostack, preserves_flags)
        );
    }

    if (gsbase == 0) {
        panic!("mpk_set_return_stack_syscall_entry: gsbase is not set");
    }

    
0

}



pub extern "C" fn mpk_make_threei_call_wrapper(
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
) -> i64 {
    threei::make_syscall(self_cageid, syscall_num, syscall_name, target_cageid, arg1, arg1_cageid, arg2, arg2_cageid, arg3, arg3_cageid, arg4, arg4_cageid, arg5, arg5_cageid, arg6, arg6_cageid)
}
