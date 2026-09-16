
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
use crate::lind_mpk::execute::{setup_supervisor_stack, current_tid, setup_gs_context, CAGE_STACK_GUARD};
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

// glibc's x86-64 jmp_buf stores these registers in this order. The buffer is
// created on the supervisor stack so clone3's copied stack can use the same address.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MpkJumpBuffer {
    rbx: libc::c_long,
    rbp: libc::c_long,
    r12: libc::c_long,
    r13: libc::c_long,
    r14: libc::c_long,
    r15: libc::c_long,
    rsp: libc::c_long,
    rip: libc::c_long,
}

unsafe extern "C" {
    fn setjmp(env: *mut libc::c_long) -> c_int;
    fn longjmp(env: *mut libc::c_long, value: c_int) -> !;
}

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

#[cfg(target_arch = "x86_64")]
fn current_pointer_guard() -> u64 {
    let stack_guard: u64;
    unsafe {
        asm!(
            "mov %fs:0x30, {stack_guard}",
            stack_guard = out(reg) stack_guard,
            options(nostack, preserves_flags, att_syntax)
        );
    }
    stack_guard
}

#[cfg(target_arch = "x86_64")]
fn relocate_mangled_stack_pointer(
    pointer: libc::c_long,
    delta: isize,
    source_pointer_guard: u64,
    target_pointer_guard: u64,
) -> libc::c_long {
    let mangled = pointer as u64;
    let unmangled = mangled.rotate_right(17) ^ source_pointer_guard;
    let relocated = unmangled.wrapping_add(delta as u64);
    let remangled = (relocated ^ target_pointer_guard).rotate_left(17);
    mpk_debug(format!(
        "ptr mangle: mangled={:#x}, source_guard={:#x}, unmangled={:#x}, delta={:#x}, relocated={:#x}, target_guard={:#x}, remangled={:#x}",
        mangled,
        source_pointer_guard,
        unmangled,
        delta,
        relocated,
        target_pointer_guard,
        remangled,
    ));
    remangled as libc::c_long
}

fn mpk_clone_thread_entry(
    tid_sender: std::sync::mpsc::SyncSender<libc::pid_t>,
    new_gs_data_addr: usize,
    new_stack_base_addr: usize,
    jump_buffer_addr: usize,
    old_stack_base: usize,
    parent_pointer_guard: u64,
    registered_cage_ids: std::collections::HashSet<u64>,
    thread_info: Arc<MpkThreadInfo>,
) -> i32 {
    let tid = current_tid();
    let _ = tid_sender.send(tid);
    let new_gs_data = new_gs_data_addr as *mut MPKSupervisorCtxStack;
    unsafe {
        (*new_gs_data).os_tid = tid as u64;
        let result = libc::syscall(
            libc::SYS_arch_prctl,
            0x1001 as libc::c_long,
            new_gs_data as u64,
        );
        if result != 0 {
            lind_debug_panic(&format!(
                "mpk_clone: arch_prctl(ARCH_SET_GS) failed: {}",
                std::io::Error::last_os_error()
            ));
            return -EINVAL;
        }
    }

    for cage_id in registered_cage_ids {
        let Some(cage) = get_cage(cage_id) else {
            lind_debug_panic(&format!("mpk_clone: cage {} not found", cage_id));
            return -EINVAL;
        };
        let mut cage_info = MpkCageThreadInfo {
            thread_info: Arc::clone(&thread_info),
            grate_cage_id: cage_id,
            stack_addr: 0,
            stack_base: 0,
            stack_size: 0,
        };
        cage_info.thread_info.register_cage(cage_id);
        if let Err(error) = cage_info.allocate_grate_stack() {
            lind_debug_panic(&format!(
                "mpk_clone: grate stack allocation failed for tid={}: {}",
                tid, error
            ));
            return -EINVAL;
        }
        let runtime_info = cage.runtime_info.read();
        let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() else {
            lind_debug_panic("mpk_clone: cage runtime changed while creating thread");
            return -EINVAL;
        };
        mpk_info.threads.write().insert(tid, cage_info);
    }

    let copied_jump_buffer =
        (new_stack_base_addr + (jump_buffer_addr - old_stack_base)) as *mut MpkJumpBuffer;
    unsafe {
        let child_pointer_guard = current_pointer_guard();
        let stack_delta = new_stack_base_addr as isize - old_stack_base as isize;
        //fix up all supervisor stack entries with the new stack base.
        assert!(
            (*new_gs_data).current_context > 0,
            "mpk_clone: supervisor context stack is empty"
        );
        //for all contexts, offset supervisor stack pointers with the stack delta
        for i in 0..(*new_gs_data).current_context {
            mpk_debug(&format!(
                "mpk_clone: adjusting supervisor stack pointer for context {}: super_rsp = {:#x}, new super_rsp = {:#x}",
                i, (*new_gs_data).contexts[i].super_rsp, ((*new_gs_data).contexts[i].super_rsp as isize + stack_delta) as u64
            ));
            (*new_gs_data).contexts[i].super_rsp = ((*new_gs_data).contexts[i].super_rsp as isize + stack_delta) as u64;
        }

        (*copied_jump_buffer).rsp = relocate_mangled_stack_pointer(
            (*copied_jump_buffer).rsp,
            stack_delta,
            parent_pointer_guard,
            child_pointer_guard,
        );
        (*copied_jump_buffer).rbp = relocate_mangled_stack_pointer(
            (*copied_jump_buffer).rbp,
            stack_delta,
            parent_pointer_guard,
            child_pointer_guard,
        );
        (*copied_jump_buffer).rip = relocate_mangled_stack_pointer(
            (*copied_jump_buffer).rip,
            0,
            parent_pointer_guard,
            child_pointer_guard,
        );
        longjmp(copied_jump_buffer as *mut libc::c_long, 1);
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
    //do setjmp here to save the current execution context for the child thread
    //then return inner_mpk_clone_syscall_entry()
    //the longjmp back will just return 0;

    let mut jump_buffer = MpkJumpBuffer::default();

    let jump_result = unsafe {
        setjmp(&mut jump_buffer as *mut MpkJumpBuffer as *mut libc::c_long)
    };
    if jump_result != 0 {
        return 0;
    }

    let result = inner_mpk_clone_syscall_entry(
        cageid,
        _clone_arg,
        _clone_arg_cageid,
        _parent_cageid,
        _arg2_cageid,
        _child_cageid_hint,
        _arg3_cageid,
        &mut jump_buffer,
        _arg4_cageid,
        _arg5,
        _arg5_cageid,
        _arg6,
        _arg6_cageid,
    );
    result
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
pub extern "C" fn inner_mpk_clone_syscall_entry(
    cageid: u64, //This is rawposix' cage id as it performs make_syscall
    _clone_arg: u64,
    _clone_arg_cageid: u64,
    _parent_cageid: u64,
    _arg2_cageid: u64,
    _child_cageid_hint: u64,
    _arg3_cageid: u64,
    jump_buffer: *mut MpkJumpBuffer,
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
        //CLONE_PARENT_SETTID: Store TID at parent_tid
        //CLONE_CHILD_SETTID: Store TID at child_tid
        //CLONE_CHILD_CLEARTID: Clear TID and futex wake at child_tid on thread exit

        let parent_tid_ptr = args.parent_tid;
        let child_tid_ptr = args.child_tid;
        let set_parent_tid = (args.flags & (CLONE_PARENT_SETTID as u64)) != 0;
        let set_child_tid = (args.flags & (CLONE_CHILD_SETTID as u64)) != 0;
        let clear_child_tid = (args.flags & (CLONE_CHILD_CLEARTID as u64)) != 0;

   
        //Ignored/set flags are ()
        //CLONE_IO: The IO scheduler is out of scope for Lind.  
        //CLONE_FILES: not yet supported, needs to be implemented in Rawposix/fdtables
        //CLONE_FS: not yet supported, needs to be implemented in Rawposix/fdtables
        //CLONE_THREAD: always set for threads
        //CLONE_SIGHAND: not yet supported
        //CLONE_SYSVSEM: not yet supported
        args.flags |= ((CLONE_THREAD | CLONE_FILES | CLONE_FS | CLONE_SIGHAND | CLONE_SYSVSEM) as u64);

        //Ignored/cleared flags are (Most of these are simply out of scope as they are Linux specific, not POSIX)
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
        //CLONE_PIDFD: not supported
        //CLONE_PTRACE: not supported
        //CLONE_STOPPED: not supported
        //CLONE_UNTRACED: not supported
        //CLONE_VFORK: not supported
        args.flags &= !((CLONE_CLEAR_SIGHAND 
            | CLONE_DETACHED | CLONE_INTO_CGROUP | CLONE_NEWCGROUP | CLONE_NEWIPC | CLONE_NEWNET
            | CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWUSER | CLONE_NEWUTS | CLONE_PARENT
            | CLONE_PIDFD | CLONE_PTRACE | CLONE_UNTRACED
            | CLONE_VFORK) as u64);
        args.pidfd = 0;

        //still unsupported flags?
        if (args.flags & !((CLONE_VM | CLONE_SETTLS | CLONE_THREAD | CLONE_FILES | CLONE_FS | CLONE_SIGHAND | CLONE_SYSVSEM | CLONE_PARENT_SETTID | CLONE_CHILD_SETTID | CLONE_CHILD_CLEARTID) as u64)) != 0 {
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

            if set_parent_tid && parent_tid_ptr != 0 {
                unsafe {
                    *(parent_tid_ptr as *mut i32) = next_tid as i32;
                }
            }
            if set_child_tid && child_tid_ptr != 0 {
                unsafe {
                    *(child_tid_ptr as *mut i32) = next_tid as i32;
                }
            }


            let parent_tid = current_tid();
            let parent_thread_info = mpk_info
                .threads
                .read()
                .get(&parent_tid)
                .map(|thread| Arc::clone(&thread.thread_info))
                .expect("mpk_clone: parent thread is not registered");
            let registered_cage_ids = parent_thread_info.cage_ids.lock().unwrap().clone();
            let jump_buffer_addr = jump_buffer as usize;
            let old_stack_base = parent_thread_info.supervisor_stack_base;
            let stack_size = parent_thread_info.supervisor_stack_size;
            let context_pages_size = parent_thread_info.context_pages_size;
            let parent_pointer_guard = current_pointer_guard();

            if stack_size == 0 || context_pages_size == 0 {
                lind_debug_panic("mpk_clone: parent thread has no supervisor context");
                return -EINVAL;
            }

            let new_context_pages = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    context_pages_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            let new_stack_base = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    stack_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if new_context_pages == libc::MAP_FAILED || new_stack_base == libc::MAP_FAILED {
                if new_context_pages != libc::MAP_FAILED {
                    unsafe { libc::munmap(new_context_pages, context_pages_size) };
                }
                if new_stack_base != libc::MAP_FAILED {
                    unsafe { libc::munmap(new_stack_base, stack_size) };
                }
                lind_debug_panic("mpk_clone: failed to duplicate thread context");
                return -EINVAL;
            }

            let new_stack_base_addr = new_stack_base as usize;

            unsafe {
                std::ptr::copy_nonoverlapping(
                    parent_thread_info.context_pages as *const u8,
                    new_context_pages as *mut u8,
                    context_pages_size,
                );
                std::ptr::copy_nonoverlapping(
                    (parent_thread_info.supervisor_stack_base as u64 + CAGE_STACK_GUARD as u64) as *const u8,
                    (new_stack_base as u64 + CAGE_STACK_GUARD as u64) as *mut u8,
                    (stack_size - CAGE_STACK_GUARD),
                );
                libc::mprotect(new_stack_base, CAGE_STACK_GUARD, libc::PROT_NONE);
            }

            let new_gs_data_addr = new_context_pages as usize;
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
            let new_cage_data_addr = new_context_pages as usize + page_size;
            let clear_child_tid_addr = if clear_child_tid { child_tid_ptr } else { 0 };
            let thread_info = Arc::new(MpkThreadInfo {
                gs_data: new_gs_data_addr as *mut MPKSupervisorCtxStack,
                cage_data: new_cage_data_addr as *mut crate::lind_mpk::RuntimeInfo::MPKCageCtxStack,
                context_pages: new_context_pages as usize,
                context_pages_size,
                supervisor_stack_base: new_stack_base as usize,
                supervisor_stack_size: stack_size,
                cage_ids: Mutex::new(std::collections::HashSet::new()),
                child_tid: AtomicU64::new(clear_child_tid_addr),
            });

            // Fix up the top grate's return stack with the caller-provided stack.
            unsafe {
                let cage_data = &mut *thread_info.cage_data;
                assert!(
                    cage_data.current_context > 0,
                    "mpk_clone: cage context stack is empty"
                );
                cage_data.contexts[cage_data.current_context - 1].rsp = args.stack + args.stack_size;
            }


            // Spawn the child on a normal Rust stack, then switch it to the copied
            // supervisor stack before resuming at the saved setjmp return point.
            let (tid_sender, tid_receiver) = std::sync::mpsc::sync_channel(1);
            let handle = thread::spawn(move || {
                mpk_clone_thread_entry(
                    tid_sender,
                    new_gs_data_addr,
                    new_stack_base_addr,
                    jump_buffer_addr,
                    old_stack_base,
                    parent_pointer_guard,
                    registered_cage_ids,
                    thread_info,
                )
            });


            //update parent thread's return value
            let child_os_tid = tid_receiver
                .recv()
                .expect("mpk_clone: child thread exited before reporting its tid");
            cage.os_tid_map.insert(next_tid as i32, child_os_tid as i64);

            return next_tid as i32;

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
                
                // MPK has no Wasmtime epoch; pass a pointer to a static zero so
                // lind_signal_init stores a valid (disabled) epoch handler address.
                let epoch_pointer: *mut u64 = MPK_EPOCH.as_ptr();        
                
                // initialize the signal for the main thread of forked cage
                lind_signal_init(
                    child_cageid,
                    epoch_pointer,
                    THREAD_START_ID, //this is the first thread of the new cage
                    true, /* this is the main thread */
                );

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
                    child_tid: AtomicU64::new(0),
                });

                let mut registered_cage_ids = parent_registered_cages;
                registered_cage_ids.insert(child_cageid);

                for cage_id in &registered_cage_ids {
                    let mut cage_info = MpkCageThreadInfo {
                        thread_info: Arc::clone(&thread_info),
                        grate_cage_id: *cage_id,
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
            
            let mut cage_tid = 0;
            let cage_pid = mpk_info.pid;
            if cage_pid != 0 {
                // Retrieve the thread ID from MPKSupervisorCtxStack and store it in cage_tid, we cant use gettid() directly.
                unsafe {
                    asm!(
                        "mov %gs:{gs_super_os_tid_offset}, {cage_tid}",
                        cage_tid = out(reg) cage_tid,
                        gs_super_os_tid_offset = const GS_SUPER_OS_TID,
                        options(att_syntax)
                    );
                }
            }
            else {
                //This cage is running in the same process, so we need to clean up the linker state.
                // Remove the exiting thread's MpkThreadInfo from the map; Drop frees its
                // supervisor stack and GsSegmentData.
                cage_tid = current_tid();
            }

            let lind_tid = cage
                .os_tid_map
                .iter()
                .find(|entry| *entry.value() == cage_tid as i64)
                .map(|entry| *entry.key())
                .expect("mpk_exit: current OS thread is not registered in os_tid_map");
            let is_last = cage::signal::lind_thread_exit(exiting_cageid, lind_tid as u64);
            mpk_debug(format!("mpk_exit: cage {} is_last_thread: {}", exiting_cageid, is_last));            
            if is_last {               
                
                let my_pid = unsafe { libc::getpid() };
                
                mpk_debug(format!("mpk_exit: cage_pid={}, my_pid={}", cage_pid, my_pid));
                
                if cage_pid != 0 {
                    // If the cage has a non-zero PID and it's different from our own,
                    // kill that process (it's a forked child)
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
                else {

                    
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
                
                cage::cage_finalize(exiting_cageid);
                
                // Decrement the cage counter
                if let Some(lind_manager) = LIND_MANAGER.get() {
                    //ultimate teardown if this was the last cage
                    if lind_manager.decrement_and_is_zero() {
                        mpk_debug(format!("mpk_exit: no more cages, exiting (exit_group) with status {}", exit_status));
                        //no more cages => exitgroup
                        unsafe { libc::syscall(libc::SYS_exit_group, exit_status); }
                    }
                }
            }
            else {
                
                let thread_info = mpk_info.threads.write().remove(&cage_tid).map(|cage_thread_info| {
                    Arc::clone(&cage_thread_info.thread_info)
                });
    
                if let Some(thread_info) = thread_info {
                    let clear_tid_addr = thread_info.child_tid.swap(0, Ordering::SeqCst);
                    if clear_tid_addr != 0 {
                        unsafe {
                            let atomic_tid = clear_tid_addr as *const AtomicI32;
                            (*atomic_tid).store(0, Ordering::Release);
                            //TODO: can we do this in rawposix?
                            libc::syscall(
                                libc::SYS_futex,
                                clear_tid_addr,
                                libc::FUTEX_WAKE,
                                1,
                                std::ptr::null::<libc::c_void>(),
                                std::ptr::null::<libc::c_void>(),
                                0,
                            );
                        }
                    }
    
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
    
                    // We are still executing on this OS thread's supervisor stack, which
                    // `thread_info` owns. Hand the last reference to the reaper thread so
                    // it is only dropped (and the stack unmapped) once this OS thread has
                    // actually exited.
                    crate::lind_mpk::RuntimeInfo::queue_thread_info_for_reap(cage_tid, thread_info);
                }
                else {
                    mpk_debug(format!("mpk_exit: no thread info found for cage {}", exiting_cageid));
                }
            }
            
            
        } else {
            mpk_debug(format!("mpk_exit: cage {} has no MPKRuntimeInfo", exiting_cageid));
        }
        
        
    } else {
        mpk_debug(format!("mpk_exit: cage {} not found", exiting_cageid));
    }
    

    mpk_debug(format!("mpk_exit: cage {} cleanup complete, exiting (exit, not exit_group) with status {}", exiting_cageid, exit_status));

    

    //Here the thread is terminated. (exit does not return in this implementation)
    //it is assumed that there are no references to rust objects stored on the supervisor stack at this point.
    unsafe {
        libc::syscall(libc::SYS_exit, exit_status); // One test expects lind to exit with the cages exit code
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
