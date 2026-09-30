//! Signal and lifecycle support for native MPK cages.
///TODO
/// - find better solution for map_tid_to_cage
/// - check ordering of context stack increment: Are there any races with signal handling?
/// - prevent deadlocks by managing the signal mask carefully
use crate::lind_mpk::RuntimeInfo::{MPKCageCtxStack, MPKRuntimeInfo, MPKSupervisorCtxStack, LIND_MPK_MAX_CONTEXTS};
use cage::memory::vmmap::VmmapOps;
use cage::get_cage;
use std::arch::asm;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{__cpuid, __cpuid_count};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{OnceLock, RwLock};
use std::sync::atomic::Ordering;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{PAGESHIFT, PROT_EXEC, PROT_NONE};
use sysdefs::constants::lind_platform_const::{INIT_CAGEID, RAWPOSIX_CAGEID, THREEI_CAGEID, UNUSED_ARG, UNUSED_ID};
use sysdefs::constants::syscall_const::KILL_SYSCALL;
use sysdefs::constants::sys_const::{SIGCONT, SIGKILL, SIGSEGV, SIGSTOP};
use sysdefs::constants::{SIG_DFL, SIG_IGN, SignalDefaultHandler};
use threei::threei_const::RUNTIME_TYPE_MPK;

const MPK_CAGE_WINDOW_SHIFT: usize = 32; // 4 GiB windows
const MPK_CAGE_WINDOW_SIZE: usize = 1usize << MPK_CAGE_WINDOW_SHIFT;
const MPK_CAGE_WINDOW_MASK: usize = !(MPK_CAGE_WINDOW_SIZE - 1);
const SIGNAL_ALT_STACK_SIZE: usize = libc::SIGSTKSZ as usize;
const SIGNAL_STACK_RED_ZONE: usize = 128;
#[cfg(target_arch = "x86_64")]
const FP_XSTATE_MAGIC1: u32 = 0x4650_5853;
#[cfg(target_arch = "x86_64")]
const FPSTATE_SW_RESERVED_OFFSET: usize = 464;
#[cfg(target_arch = "x86_64")]
const XFEATURE_PKRU: u32 = 9;
#[cfg(target_arch = "x86_64")]
const XFEATURE_PKRU_MASK: u64 = 1u64 << XFEATURE_PKRU;

// Keyed by 4GiB-aligned virtual base (window start) -> cageid.
static MPK_MEMORY_INDEX: OnceLock<RwLock<HashMap<usize, u64>>> = OnceLock::new();
static SIGNAL_ALT_STACK_PTR: OnceLock<usize> = OnceLock::new();

fn memory_index() -> &'static RwLock<HashMap<usize, u64>> {
    MPK_MEMORY_INDEX.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_mpk_memory_region(cageid: u64, base: usize, size: usize) {
    if base == 0 || size == 0 {
        return;
    }

    let mut index = memory_index().write().unwrap();
    index.retain(|_, mapped_cageid| *mapped_cageid != cageid);

    let first_window = base & MPK_CAGE_WINDOW_MASK;
    let end = base.saturating_add(size.saturating_sub(1));
    let last_window = end & MPK_CAGE_WINDOW_MASK;

    let mut window = first_window;
    loop {
        index.insert(window, cageid);
        if window == last_window {
            break;
        }
        window = window.saturating_add(MPK_CAGE_WINDOW_SIZE);
    }
}

fn cage_from_address(addr: usize) -> Option<u64> {
    let window = addr & MPK_CAGE_WINDOW_MASK;
    let cageid = {
        let index = memory_index().read().unwrap();
        *index.get(&window)?
    };

    // Re-validate with the current runtime info to avoid stale index hits.
    let cage = get_cage(cageid)?;
    let runtime = cage.runtime_info.read();
    let info = runtime.as_any().downcast_ref::<MPKRuntimeInfo>()?;
    let info_base = info.memory_base as usize;
    let info_end = info_base.saturating_add(info.memory_size);
    if addr >= info_base && addr < info_end {
        Some(cageid)
    } else {
        None
    }
}

fn current_tid() -> libc::pid_t {
    unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t }
}

fn cage_from_supervisor_ctx_top() -> Option<u64> {
    #[cfg(not(target_arch = "x86_64"))]
    {
        None
    }

    #[cfg(target_arch = "x86_64")]
    unsafe {
        let gs_data = gs_base() as *const MPKSupervisorCtxStack;
        if gs_data.is_null() {
            return None;
        }

        let depth = (*gs_data).current_context;
        if depth > LIND_MPK_MAX_CONTEXTS {
            return None;
        }

        let cageid = (*gs_data).contexts[depth].cage_id;
        if cageid == 0 {
            None
        } else {
            Some(cageid)
        }
    }
}

fn with_mpk_info<R>(cageid: u64, f: impl FnOnce(&MPKRuntimeInfo) -> R) -> Option<R> {
    let cage = get_cage(cageid)?;
    let runtime = cage.runtime_info.read();
    let info = runtime.as_any().downcast_ref::<MPKRuntimeInfo>()?;
    Some(f(info))
}

fn addr_in_cage_vmmap(cageid: u64, addr: usize) -> bool {
    let Some(cage) = get_cage(cageid) else { return false };
    let vmmap = cage.vmmap.read();
    let page = vmmap.sys_to_page_num(addr);
    vmmap
        .find_page(page)
        .map(|entry| {
            let start = vmmap.page_num_to_sys(entry.page_num);
            let end = start.saturating_add(entry.npages << PAGESHIFT);
            addr >= start && addr < end
        })
        .unwrap_or(false)
}

#[cfg(target_arch = "x86_64")]
fn gs_base() -> usize {
    let base: usize;
    unsafe {
        asm!(
            "rdgsbase {out}",
            out = out(reg) base,
            options(nostack, preserves_flags, att_syntax),
        );
    }
    base
}

#[cfg(target_arch = "x86_64")]
fn current_rsp() -> usize {
    let rsp: usize;
    unsafe {
        asm!(
            "mov %rsp, {out}",
            out = out(reg) rsp,
            options(nostack, preserves_flags, att_syntax),
        );
    }
    rsp
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct FpxSwBytes {
    magic1: u32,
    extended_size: u32,
    xfeatures: u64,
    xstate_size: u32,
    _padding: [u32; 7],
}

#[cfg(target_arch = "x86_64")]
fn xsave_feature_offset(feature: u32) -> Option<usize> {
    let max_leaf = unsafe { __cpuid(0).eax };
    if max_leaf < 0xD {
        return None;
    }
    let feature_leaf = unsafe { __cpuid_count(0xD, feature) };
    Some(feature_leaf.ebx as usize)
}

#[cfg(target_arch = "x86_64")]
fn pkru_from_ucontext(uctx: *mut c_void) -> Option<u32> {
    if uctx.is_null() {
        return None;
    }

    unsafe {
        let fpstate = (*(uctx as *const libc::ucontext_t)).uc_mcontext.fpregs;
        if fpstate.is_null() {
            return None;
        }

        let sw_ptr = (fpstate as *const u8).add(FPSTATE_SW_RESERVED_OFFSET) as *const FpxSwBytes;
        let sw = &*sw_ptr;
        if sw.magic1 != FP_XSTATE_MAGIC1 || (sw.xfeatures & XFEATURE_PKRU_MASK) == 0 {
            return None;
        }

        let pkru_offset = xsave_feature_offset(XFEATURE_PKRU)?;
        let fpstate_size = sw.extended_size as usize;
        if pkru_offset.saturating_add(std::mem::size_of::<u32>()) > fpstate_size {
            return None;
        }

        let pkru_ptr = (fpstate as *const u8).add(pkru_offset) as *const u32;
        Some(std::ptr::read_unaligned(pkru_ptr))
    }
}

fn protect_mapped_memory(cageid: u64, only_executable: bool, protection: i32) {
    let Some(cage) = get_cage(cageid) else { return };
    let mut vmmap = cage.vmmap.write();
    let ranges: Vec<(usize, usize)> = vmmap
        .double_ended_iter()
        .filter(|(_, entry)| !only_executable || entry.prot & PROT_EXEC != 0)
        .map(|(_, entry)| (entry.page_num, entry.npages))
        .collect();

    for (page, npages) in ranges {
        let address = vmmap.page_num_to_sys(page);
        let length = npages << PAGESHIFT;
        unsafe {
            libc::mprotect(address as *mut c_void, length, protection);
        }
        vmmap.change_prot(page, npages, protection);
    }
}

fn set_executable_memory(cageid: u64, executable: bool) {
    let Some(cage) = get_cage(cageid) else { return };
    let mut vmmap = cage.vmmap.write();
    let ranges: Vec<(usize, usize, i32)> = vmmap
        .double_ended_iter()
        .filter(|(_, entry)| entry.maxprot & PROT_EXEC != 0)
        .map(|(_, entry)| {
            let protection = if executable {
                entry.prot | PROT_EXEC
            } else {
                entry.prot & !PROT_EXEC
            };
            (entry.page_num, entry.npages, protection)
        })
        .collect();

    for (page, npages, protection) in ranges {
        let address = vmmap.page_num_to_sys(page);
        let length = npages << PAGESHIFT;
        unsafe {
            libc::mprotect(address as *mut c_void, length, protection);
        }
        vmmap.change_prot(page, npages, protection);
    }
}

fn wake_threads(cageid: u64) {
    let _ = with_mpk_info(cageid, |info| info.state_cv.notify_all());
    if let Some(cage) = get_cage(cageid) {
        for entry in cage.os_tid_map.iter() {
            unsafe {
                libc::syscall(libc::SYS_tkill, *entry.value() as libc::pid_t, libc::SIGUSR2);
            }
        }
    }
}

extern "C" fn mpk_sigusr2_handler(_signo: i32, _info: *mut libc::siginfo_t, _uctx: *mut c_void) {
    //First check: were we in the supervisor (determine by PKRU), if yes, just return. 
    //This will cause an EINTR to be returned if the supervisor is sleeping. It can then run the signal handler on the return path.
    #[cfg(target_arch = "x86_64")]
    if matches!(pkru_from_ucontext(_uctx), Some(0)) {
        // We interrupted supervisor execution; return and let supervisor-side paths handle EINTR.
        return;
    }

    #[cfg(target_arch = "x86_64")]
    let pre_signal_rsp = unsafe {
        let uctx = _uctx as *const libc::ucontext_t;
        if uctx.is_null() {
            None
        } else {
            Some((*uctx).uc_mcontext.gregs[libc::REG_RSP as usize] as usize)
        }
    };
    #[cfg(not(target_arch = "x86_64"))]
    let pre_signal_rsp: Option<usize> = None;


    
    // If we interrupted cage execution, dispatch using the active cage on
    // top of the supervisor context stack.
    if let Some(cageid) = cage_from_supervisor_ctx_top() {
        handle_signal(cageid, pre_signal_rsp);
    }
}

extern "C" fn mpk_sigsegv_handler(_signo: i32, _info: *mut libc::siginfo_t, uctx: *mut c_void) {
    #[cfg(target_arch = "x86_64")]
    let addr_opt = unsafe {
        let uctx = uctx as *const libc::ucontext_t;
        if uctx.is_null() {
            None
        } else {
            Some((*uctx).uc_mcontext.gregs[libc::REG_RIP as usize] as usize)
        }
    };
    #[cfg(not(target_arch = "x86_64"))]
    let addr_opt: Option<usize> = None;

    if let Some(cageid) = addr_opt.and_then(cage_from_address) {
        lind_mpk_segfault_handler(cageid);
        return;
    }

    // Not an MPK cage fault; restore default behavior.
    unsafe {
        libc::signal(libc::SIGSEGV, libc::SIG_DFL);
        libc::raise(libc::SIGSEGV);
    }
}

pub fn register_os_signal_handlers() -> anyhow::Result<()> {
    unsafe {
        let alt_stack_ptr = *SIGNAL_ALT_STACK_PTR.get_or_init(|| {
            let stack = vec![0u8; SIGNAL_ALT_STACK_SIZE].into_boxed_slice();
            let ptr = stack.as_ptr() as usize;
            std::mem::forget(stack);
            ptr
        });

        let mut ss: libc::stack_t = std::mem::zeroed();
        ss.ss_sp = alt_stack_ptr as *mut c_void;
        ss.ss_flags = 0;
        ss.ss_size = SIGNAL_ALT_STACK_SIZE;
        if libc::sigaltstack(&ss, std::ptr::null_mut()) != 0 {
            anyhow::bail!("failed to install alternate signal stack");
        }

        let mut usr2: libc::sigaction = std::mem::zeroed();
        let usr2_handler: extern "C" fn(i32, *mut libc::siginfo_t, *mut c_void) =
            mpk_sigusr2_handler;
        usr2.sa_sigaction = std::mem::transmute::<
            extern "C" fn(i32, *mut libc::siginfo_t, *mut c_void),
            usize,
        >(usr2_handler);
        usr2.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&mut usr2.sa_mask);
        if libc::sigaction(libc::SIGUSR2, &usr2, std::ptr::null_mut()) != 0 {
            anyhow::bail!("failed to register SIGUSR2 handler");
        }

        let mut segv: libc::sigaction = std::mem::zeroed();
        let segv_handler: extern "C" fn(i32, *mut libc::siginfo_t, *mut c_void) =
            mpk_sigsegv_handler;
        segv.sa_sigaction = std::mem::transmute::<
            extern "C" fn(i32, *mut libc::siginfo_t, *mut c_void),
            usize,
        >(segv_handler);
        segv.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&mut segv.sa_mask);
        if libc::sigaction(libc::SIGSEGV, &segv, std::ptr::null_mut()) != 0 {
            anyhow::bail!("failed to register SIGSEGV handler");
        }
    }
    Ok(())
}

pub extern "C" fn mpk_kill_syscall_entry(
    cageid: u64,
    target_cage_arg: u64,
    target_cage_arg_cageid: u64,
    sig_arg: u64,
    sig_arg_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {

    let target_cage = target_cage_arg as i32;
    let sig = sig_arg as i32;

    if target_cage < 0 {
        return syscall_error(Errno::EINVAL, "kill", "Invalid target cage id");
    }
    if sig <= 0 || sig >= 32 {
        return syscall_error(Errno::EINVAL, "kill", "Invalid signal number");
    }

    let target_cageid = if target_cage == 0 {
        cageid
    } else {
        target_cage as u64
    };

    let Some(target_cage_obj) = get_cage(target_cageid) else {
        return syscall_error(Errno::ESRCH, "kill", "Target cage does not exist");
    };
    if target_cage_obj.runtime_type.load(Ordering::Acquire) != RUNTIME_TYPE_MPK {
        return rawposix::sys_calls::kill_syscall(
            cageid,
            target_cage_arg,
            target_cage_arg_cageid,
            sig_arg,
            sig_arg_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        ) as i32;
    }

    let delivered = match sig {
        SIGSTOP | SIGCONT | SIGKILL => send_signal(sig, target_cageid, 0),
        _ => {
            return rawposix::sys_calls::kill_syscall(
                cageid,
                target_cage_arg,
                target_cage_arg_cageid,
                sig_arg,
                sig_arg_cageid,
                arg3,
                arg3_cageid,
                arg4,
                arg4_cageid,
                arg5,
                arg5_cageid,
                arg6,
                arg6_cageid,
            ) as i32;
        }
    };

    if delivered {
        0
    } else {
        syscall_error(Errno::ESRCH, "kill", "Target cage does not exist")
    }
}

pub fn register_kill_syscall_handler() -> anyhow::Result<()> {
    let ret = threei::register_handler(
        UNUSED_ID,
        THREEI_CAGEID,
        INIT_CAGEID,
        KILL_SYSCALL as u64,
        RUNTIME_TYPE_MPK,
        RAWPOSIX_CAGEID,
        mpk_kill_syscall_entry as *const () as u64,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );

    if ret != 0 {
        anyhow::bail!("registering MPK kill handler failed: {}", ret);
    }
    Ok(())
}

/// Send a signal using MPK lifecycle semantics for STOP, CONT and KILL.
/// Other signals use Lind's normal pending-signal queue.
pub fn send_signal(signo: i32, cageid: u64, _threadid: i32) -> bool {
    let Some(cage) = get_cage(cageid) else { return false };
    match signo {
        SIGSTOP => {
            if with_mpk_info(cageid, |info| info.stopped.store(true, Ordering::Release)).is_some() {
                set_executable_memory(cageid, false);
                wake_threads(cageid);
            }
        }
        SIGCONT => {
            if with_mpk_info(cageid, |info| info.stopped.store(false, Ordering::Release)).is_some() {
                set_executable_memory(cageid, true);
                wake_threads(cageid);
            }
        }
        SIGKILL => {
            if with_mpk_info(cageid, |info| {
                info.killed.store(true, Ordering::Release);
                info.stopped.store(false, Ordering::Release);
            }).is_some() {
                cage::cage_record_exit_status(cageid, cage::ExitStatus::Signaled(SIGKILL, false));
                cage.is_dead.store(true, Ordering::Release);
                threei::EXITING_TABLE.insert(cageid);
                threei::handler_table::_rm_grate_from_handler(cageid);
                protect_mapped_memory(cageid, false, PROT_NONE);
                wake_threads(cageid);
            }
        }
        _ => return cage::signal::signal::lind_send_signal(cageid, signo),
    }
    true
}

/// Block an MPK entry point while a cage is stopped.
pub fn wait_if_stopped(cageid: u64) -> bool {
    let Some(cage) = get_cage(cageid) else { return true };
    let runtime = cage.runtime_info.read();
    let Some(info) = runtime.as_any().downcast_ref::<MPKRuntimeInfo>() else { return false };
    let mut guard = info.state_lock.lock().unwrap();
    while info.stopped.load(Ordering::Acquire) && !info.killed.load(Ordering::Acquire) {
        guard = info.state_cv.wait(guard).unwrap();
    }
    info.killed.load(Ordering::Acquire)
}

#[cfg(target_arch = "x86_64")]
unsafe fn call_handler_on_cage_stack(
    stack_top: usize,
    handler_fn: unsafe extern "C" fn(i32),
    signo: i32,
) {
    // Keep 16-byte alignment before the call to satisfy SysV ABI.
    let aligned_stack = stack_top & !0xf;
    unsafe {
        asm!(
            "mov %rsp, %r15",
            "mov {new_rsp}, %rsp",
            "call *{handler}",
            "mov %r15, %rsp",
            new_rsp = in(reg) aligned_stack,
            handler = in(reg) handler_fn,
            in("edi") signo,
            lateout("r15") _,
            clobber_abi("C"),
            options(att_syntax),
        );
    }
}

fn invoke_handler_on_cage_stack(
    cageid: u64,
    handler: u64,
    signo: i32,
    pre_signal_rsp: Option<usize>,
) -> bool {
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (cageid, handler, signo, pre_signal_rsp);
        return false;
    }

    #[cfg(target_arch = "x86_64")]
    {
        let os_tid = current_tid();
        let Some(cage) = get_cage(cageid) else {
            return false;
        };
        let runtime = cage.runtime_info.read();
        let Some(info) = runtime.as_any().downcast_ref::<MPKRuntimeInfo>() else {
            return false;
        };

        let (stack_top, cage_data) = {
            let threads = info.threads.read();
            let Some(thread) = threads.get(&os_tid) else {
                return false;
            };
            let cage_data = thread.thread_info.cage_data;
            let mut resolved_stack_top = 0usize;
            let mut do_stack_walk = true;

            if let Some(rsp_before_signal) = pre_signal_rsp {
                if addr_in_cage_vmmap(cageid, rsp_before_signal) {
                    resolved_stack_top = rsp_before_signal.saturating_sub(SIGNAL_STACK_RED_ZONE);
                    do_stack_walk = false;
                }
            } 

            if do_stack_walk && !cage_data.is_null() {
                let gs_data = gs_base() as *const MPKSupervisorCtxStack;
                let cage_ctx = cage_data as *const MPKCageCtxStack;
                let mut depth = unsafe {
                    (*gs_data)
                        .current_context
                        .min((*cage_ctx).current_context)
                        .min(LIND_MPK_MAX_CONTEXTS)
                };
                while depth > 0 {
                    let idx = depth - 1;
                    let matches_cage = unsafe { (*gs_data).contexts[idx].cage_id == cageid };
                    if matches_cage {
                        let saved_rsp = unsafe { (*cage_ctx).contexts[idx].rsp as usize };
                        if saved_rsp != 0 {
                            resolved_stack_top = saved_rsp.saturating_sub(SIGNAL_STACK_RED_ZONE);
                            break;
                        }
                    }
                    depth -= 1;
                }
            }


            if resolved_stack_top == 0 {
                // Here we know:
                // - we were not on the target cage's stack when the signal occurred
                // - there is no saved stack pointer on this thread's context stack
                // => Use the target cage stack top tracked in this thread's MpkCageThreadInfo.
                resolved_stack_top  = thread.stack_addr.load(Ordering::Acquire);
            }


            (resolved_stack_top, cage_data)
        };

        if stack_top == 0 || cage_data.is_null() {
            return false;
        }

        unsafe {
            let gs_data = gs_base() as *mut MPKSupervisorCtxStack;
            let super_current = (*gs_data).current_context;
            let cage_data = cage_data as *mut MPKCageCtxStack;
            let cage_current = (*cage_data).current_context;
            if super_current >= LIND_MPK_MAX_CONTEXTS || cage_current >= LIND_MPK_MAX_CONTEXTS {
                return false;
            }

            // Prepare both context stacks before entering the cage signal handler.
            // Supervisor stack can use a conservative decrement from current RSP.
            let supervisor_rsp = current_rsp().saturating_sub(SIGNAL_STACK_RED_ZONE) & !0xf;
            (*gs_data).contexts[super_current].super_rsp = supervisor_rsp as u64;
            (*gs_data).contexts[super_current].cage_id = cageid;

            (*cage_data).contexts[cage_current].rsp = (stack_top & !0xf) as u64;
            let handler_fn: unsafe extern "C" fn(i32) = std::mem::transmute(handler as usize);
            call_handler_on_cage_stack(stack_top, handler_fn, signo);
        }
        true
    }
}

/// Deliver all currently pending, unblocked signals for an MPK cage.
pub fn handle_signal(cageid: u64, pre_signal_rsp: Option<usize>) {
    while let Some((signo, handler, restorer)) = cage::signal::lind_get_first_signal(cageid) {
        if handler == SIG_IGN as u64 {
            restorer(cageid);
            continue;
        }
        if handler == SIG_DFL as u64 {
            match sysdefs::constants::signal_default_handler_dispatcher(signo) {
                SignalDefaultHandler::Terminate => {
                    let _ = send_signal(SIGKILL, cageid, 0);
                    return;
                }
                SignalDefaultHandler::Ignore => {
                    restorer(cageid);
                    continue;
                }
                SignalDefaultHandler::Stop => {
                    let _ = send_signal(SIGSTOP, cageid, 0);
                    restorer(cageid);
                    continue;
                }
                SignalDefaultHandler::Continue => {
                    let _ = send_signal(SIGCONT, cageid, 0);
                    restorer(cageid);
                    continue;
                }
                SignalDefaultHandler::NONEXIST => return,
            }
        }
        if !invoke_handler_on_cage_stack(cageid, handler, signo, pre_signal_rsp) {
            let handler_fn: unsafe extern "C" fn(i32) = unsafe { std::mem::transmute(handler as usize) };
            unsafe { handler_fn(signo) };
        }
        restorer(cageid);
    }
}

/// Convert an MPK protection fault into Lind SIGSEGV delivery.
pub fn lind_mpk_segfault_handler(cageid: u64) {
    if wait_if_stopped(cageid) {
        return;
    }
    if cage::signal::signal::signal_get_handler(cageid, SIGSEGV) != SIG_DFL as u64 {
        let _ = cage::signal::signal::lind_send_signal(cageid, SIGSEGV);
        handle_signal(cageid, None);
    } else {
        let _ = send_signal(SIGKILL, cageid, 0);
    }
}
