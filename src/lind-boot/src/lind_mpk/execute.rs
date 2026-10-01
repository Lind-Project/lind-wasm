




use crate::cli::CliOptions;
use wasmtime_lind_multi_process::THREAD_START_ID;
use crate::lind_mpk::loader::{
    auxv,
    build_process_auxv,
    mpk_load_ldso_and_binary_with_info,
    ProcessAuxv,
};
use crate::lind_mpk::syscalls::{
    ENABLE_INTERPOSE_PTR, LIND_MANAGER, NO_INTERPOSE,
    mpk_clone_syscall_entry, mpk_exit_syscall_entry, mpk_make_threei_call_wrapper,
};
use crate::lind_mpk::RuntimeInfo::{
    MPKCageCtxStack, MPKSupervisorContext, MPKSupervisorCtxStack, MPKCageContext,
    MPKRuntimeInfo, MpkCageThreadInfo, MpkThreadInfo, LIND_MPK_MAX_CONTEXTS,
    allocate_stack_in_vmmap,
};
use crate::shims::SyscallRuntime;
use anyhow::{Context, bail};
use cage::{VmmapBitWidth, get_cage, lind_signal_init};
use libc::{c_char, c_int, c_ulong, c_void};
use std::arch::asm;
use std::sync::atomic::{Ordering, AtomicU64};
use std::env;
use std::ffi::CStr;
use sysdefs::constants::syscall_const::{CLONE3_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL};
/// Minimal reproduction of the `link_map` struct from `<link.h>`.
/// The libc crate does not expose this type, so we define only the fields we
/// actually need. The layout matches the glibc ABI on x86-64 Linux.
#[repr(C)]
struct LinkMap {
    l_addr: c_ulong,
    l_name: *const c_char,
    l_ld: *mut c_void,
    l_next: *mut LinkMap,
    l_prev: *mut LinkMap,
}
use std::sync::Arc;
use wasmtime_lind_utils::LindCageManager;
use sysdefs::constants::lind_platform_const::{UNUSED_ID,  UNUSED_ARG, WASMTIME_CAGEID, RAWPOSIX_CAGEID};
use wasmtime_lind_multi_process::CAGE_START_ID;
use threei::threei_const;
use crate::lind_mpk::trampoline::{
    grate_callback_trampoline, register_mpk_handler_for_cage,
};

// Import the type alias from RuntimeInfo module
use crate::lind_mpk::RuntimeInfo::EnableInterposeF;

// dlinfo request codes not yet exposed by the libc crate.
const RTLD_DI_LMID: c_int = 1;
const RTLD_DI_LINKMAP: c_int = 2;

// 4 GB virtual address space reserved for each cage with MAP_NORESERVE
// (no swap space is committed until pages are actually touched).
const MPK_MEMORY_SIZE: usize = 4 * 1024 * 1024 * 1024;

// Stack used to call the cage's own `main`, allocated out of the cage's
// own vmmap so it is backed by memory the guest owns.
const CAGE_STACK_SIZE: usize = 8 * 1024 * 1024;
pub const CAGE_STACK_GUARD: usize = 4096;

const DEFAULT_LDSO_PATH: &str = "/lib/ld.so";

/// C main entrypoint signature.
type MainFn = unsafe extern "C" fn(c_int, *const *const c_char, *const *const c_char) -> c_int;
type ExitFn = unsafe extern "C" fn(c_int) -> !;

// Size of the supervisor stack for each thread's syscall interposition.
// The first page is PROT_NONE as a guard against supervisor stack overflow.
const SUPERVISOR_STACK_SIZE: usize = 2 * 1024 * 1024; // 2 MiB usable
const SUPERVISOR_STACK_GUARD: usize = 4096;            // one guard page

// arch_prctl code for setting the GS base register.
// Not always exported by the libc crate, so defined explicitly.
const ARCH_SET_GS: c_int = 0x1001;

// MPK has no Wasmtime epoch handler, so lind_signal_init receives a pointer to
// this static zero, matching the disable_signals behaviour used in wasmtime.
static MPK_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Allocates and initializes the per-thread GS and cage context pages, then
/// installs the GS base for the calling thread.
pub(super) fn setup_gs_context(
    cageid: u64,
    os_tid: libc::pid_t,
) -> anyhow::Result<(*mut MPKSupervisorCtxStack, *mut MPKCageCtxStack, usize, usize)> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
    if page_size == 0 {
        bail!("sysconf(_SC_PAGESIZE) failed");
    }
    let context_pages_size = page_size.checked_mul(2).context("context page size overflow")?;
    let context_pages = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            context_pages_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if context_pages == libc::MAP_FAILED {
        bail!("mmap for MPK context pages failed: {}", std::io::Error::last_os_error());
    }

    let gs_ptr = context_pages as *mut MPKSupervisorCtxStack;
    let cage_ptr = (context_pages as usize + page_size) as *mut MPKCageCtxStack;
    unsafe {
        std::ptr::write(gs_ptr, MPKSupervisorCtxStack {
            current_context: 0,
            os_tid: os_tid as u64,
            contexts: [MPKSupervisorContext::default(); LIND_MPK_MAX_CONTEXTS],
        });
        std::ptr::write(cage_ptr, MPKCageCtxStack {
            current_context: 0,
            contexts: [MPKCageContext::default(); LIND_MPK_MAX_CONTEXTS],
        });
    }

    let ret = unsafe {
        libc::syscall(
            libc::SYS_arch_prctl,
            ARCH_SET_GS as libc::c_long,
            gs_ptr as u64,
        )
    };
    if ret != 0 {
        unsafe { libc::munmap(context_pages, context_pages_size) };
        bail!("arch_prctl(ARCH_SET_GS) failed: {}", std::io::Error::last_os_error());
    }

    Ok((gs_ptr, cage_ptr, context_pages as usize, context_pages_size))
}

/// Allocates a supervisor stack and returns a `MpkThreadInfo` to be registered
/// in `MPKRuntimeInfo::threads`. GS context setup is performed separately by
/// `setup_gs_context` and can also be called independently.
///
/// The custom glibc assembly (`syscall-interpose.h`) switches to the supervisor stack
/// on every intercepted syscall by reading gs:8. This function must be called before
/// `__enable_syscall_interpose` so that GS is valid when the first interposed syscall
/// fires.
///
/// Steps:
/// 1. `mmap` a region with a `PROT_NONE` guard page at the bottom.
/// 2. Allocate a `GsSegmentData` (via `Box`), writing the stack top to `supervisor_rsp`.
/// 3. `arch_prctl(ARCH_SET_GS, gs_ptr)` — installs GS for the calling thread only.
/// 4. Return `MpkThreadInfo`; the caller inserts it into `MPKRuntimeInfo::threads` keyed
///    by the current OS thread ID so it is freed when the cage or thread exits.
///
/// For fork-based cage isolation, child processes inherit the parent thread's GS value
/// and a copy of the mapped regions via fork's address-space duplication, so no
/// additional call is needed in the forked child.  For CLONE_VM in-process threads,
/// each new thread must call this function before its first intercepted syscall.
pub(super) fn setup_supervisor_stack(cageid: u64, os_tid: libc::pid_t) -> anyhow::Result<MpkThreadInfo> {
    let (gs_ptr, cage_ptr, context_pages, context_pages_size) =
        setup_gs_context(cageid, os_tid)?;

    let total_size = SUPERVISOR_STACK_GUARD + SUPERVISOR_STACK_SIZE;

    let stack_base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            total_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if stack_base == libc::MAP_FAILED {
        unsafe {
            libc::munmap(context_pages as *mut c_void, context_pages_size)
        };
        bail!("mmap for supervisor stack failed: {}", std::io::Error::last_os_error());
    }

    // Guard page: lowest page is PROT_NONE to catch supervisor stack overflow.
    let ret = unsafe { libc::mprotect(stack_base, SUPERVISOR_STACK_GUARD, libc::PROT_NONE) };
    if ret != 0 {
        unsafe { libc::munmap(stack_base, total_size) };
        unsafe {
            libc::munmap(context_pages as *mut c_void, context_pages_size)
        };
        bail!("mprotect for supervisor stack guard failed: {}", std::io::Error::last_os_error());
    }

    // Stack top is at the high end of the allocation (x86-64 stack grows down).
    let stack_top = (stack_base as usize + total_size) as u64;

    unsafe {
        (*gs_ptr).contexts[0] = MPKSupervisorContext {
            super_rsp: stack_top,
            cage_id: cageid,
            pkru: 0,
        };
    }

    mpk_debug(format!(
        "supervisor stack (tid {}): guard=[{:p}..{:#x}], usable=[{:#x}..{:#x}], gs_data={:#x}",
        current_tid(),
        stack_base, stack_base as usize + SUPERVISOR_STACK_GUARD,
        stack_base as usize + SUPERVISOR_STACK_GUARD, stack_top,
        gs_ptr as usize,
    ));

    Ok(MpkThreadInfo {
        gs_data: gs_ptr,
        cage_data: cage_ptr,
        context_pages,
        context_pages_size,
        supervisor_stack_base: stack_base as usize,
        supervisor_stack_size: total_size,
        cage_ids: std::sync::Mutex::new(std::collections::HashSet::new()),
        exit_stack: std::sync::atomic::AtomicUsize::new(current_host_rsp()),
        child_tid: std::sync::atomic::AtomicU64::new(0),
    })
}

/// Allocates a fresh supervisor stack for a new thread by **copying** the usable
/// contents of `parent`'s supervisor stack, and returns a ready-to-register
/// `MpkThreadInfo`.
///
/// # Why a copy, not a blank stack
///
/// When `clone3` is called from within the interposition handler, execution is already
/// on the supervisor stack.  The call frames from the interpose entry through
/// `lind_syscall_handler` → `mpk_clone_syscall_entry` are live on that stack.
/// The new thread created by `clone3` must unwind those same frames to return back
/// to the application, so it needs its own private copy of the current stack contents.
/// The copy is made before `clone3` is called so that both the parent and the child
/// thread have independent, identical stack images to unwind from.
///
/// # What gets copied
///
/// The entire usable region (guard-page end → stack top) is copied.  The new
/// `GsSegmentData::supervisor_rsp` points to the top of the new region; when the
/// new thread installs GS and `clone3` returns `0`, its RSP is at the same offset
/// below the top as the parent's, so all return addresses and saved registers are
/// valid relative to the new stack base.
///
/// # GS is not installed
///
/// Like `setup_supervisor_stack_for_thread`, this function does **not** call
/// `arch_prctl(ARCH_SET_GS, ...)`.  The new thread must install GS itself after
/// `clone3` returns `0`, before the next intercepted syscall fires.
///
/// Caller responsibilities:
/// - Insert the returned `MpkThreadInfo` into `MPKRuntimeInfo::threads` under the
///   new thread's OS tid.
/// - From within the new thread, call `arch_prctl(ARCH_SET_GS, gs_data_ptr)`.
// pub(super) fn copy_supervisor_stack_for_thread(parent: &MpkThreadInfo) -> anyhow::Result<MpkThreadInfo> {
//     let total_size = parent.supervisor_stack_size;

//     let new_stack_base = unsafe {
//         libc::mmap(
//             std::ptr::null_mut(),
//             total_size,
//             libc::PROT_READ | libc::PROT_WRITE,
//             libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
//             -1,
//             0,
//         )
//     };
//     if new_stack_base == libc::MAP_FAILED {
//         bail!("mmap for cloned supervisor stack failed: {}", std::io::Error::last_os_error());
//     }

//     // Copy the usable portion of the parent's supervisor stack.
//     // The guard page is left as zeroed memory for now; mprotect below makes it PROT_NONE.
//     let usable_size = total_size - SUPERVISOR_STACK_GUARD;
//     let src = (parent.supervisor_stack_base + SUPERVISOR_STACK_GUARD) as *const u8;
//     let dst = (new_stack_base as usize + SUPERVISOR_STACK_GUARD) as *mut u8;
//     unsafe { std::ptr::copy_nonoverlapping(src, dst, usable_size) };

//     // Guard page at the bottom of the new stack.
//     let ret = unsafe { libc::mprotect(new_stack_base, SUPERVISOR_STACK_GUARD, libc::PROT_NONE) };
//     if ret != 0 {
//         unsafe { libc::munmap(new_stack_base, total_size) };
//         bail!("mprotect for cloned supervisor stack guard failed: {}", std::io::Error::last_os_error());
//     }

//     let new_stack_top = (new_stack_base as usize + total_size) as u64;

//     let mut gs_data = Box::new(GsSegmentData {
//         current_context: std::ptr::null_mut(),
//         supervisor_rsp: new_stack_top,
//         os_tid: current_tid() as u64,
//         contexts: [Default::default(); crate::lind_mpk::RuntimeInfo::LIND_MPK_MAX_CONTEXTS],
//     });
//     gs_data.current_context = gs_data.contexts.as_mut_ptr();

//     mpk_debug(format!(
//         "cloned supervisor stack (tid {}): src=[{:#x}..{:#x}] -> dst=[{:p}..{:#x}], gs_data={:#x} (GS not yet installed)",
//         current_tid(),
//         parent.supervisor_stack_base + SUPERVISOR_STACK_GUARD,
//         parent.supervisor_stack_base + total_size,
//         new_stack_base, new_stack_base as usize + total_size,
//         &*gs_data as *const GsSegmentData as u64,
//     ));

//     Ok(MpkThreadInfo {
//         gs_data,
//         supervisor_stack_base: new_stack_base as usize,
//         supervisor_stack_size: total_size,
//     })
// }


/// Returns the OS-level thread ID of the calling thread (Linux `gettid`).
pub(super) fn current_tid() -> libc::pid_t {
    unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t }
}

fn mpk_debug_enabled() -> bool {
    env::var_os("LIND_MPK_DEBUG").is_some()
}

fn mpk_debug(message: impl AsRef<str>) {
    if mpk_debug_enabled() {
        eprintln!("[lind-mpk] {}", message.as_ref());
    }
}

fn merge_host_ld_env_vars(vars: &mut Vec<(String, Option<String>)>) {
    let existing: std::collections::HashSet<String> =
        vars.iter().map(|(k, _)| k.clone()).collect();

    for (key, value) in std::env::vars() {
        if key.starts_with("LD_") && !existing.contains(&key) {
            vars.push((key, Some(value)));
        }
    }
}

fn ensure_ld_library_path_contains_lib_elf(vars: &mut Vec<(String, Option<String>)>) {
    const ELF_LIB_DIR: &str = "/lib/elf";

    if let Some((_, value)) = vars.iter_mut().find(|(key, _)| key == "LD_LIBRARY_PATH") {
        let mut current = value.clone().unwrap_or_default();
        let has_elf_lib_dir = current.split(':').any(|segment| segment == ELF_LIB_DIR);
        if !has_elf_lib_dir {
            if current.is_empty() {
                current.push_str(ELF_LIB_DIR);
            }
            else {
                current.push(':');
                current.push_str(ELF_LIB_DIR);
            }
            *value = Some(current);
        }
    }
    else {
        vars.push(("LD_LIBRARY_PATH".to_string(), Some(ELF_LIB_DIR.to_string())));
    }
}

#[cfg(target_arch = "x86_64")]
fn current_host_rsp() -> usize {
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

#[cfg(not(target_arch = "x86_64"))]
fn current_host_rsp() -> usize {
    0
}

/// Writes `args` and `envs` onto the cage's own stack (`stack_top`, growing
/// down), building the argv/envp pointer arrays and their backing strings
/// entirely within memory the cage owns. Returns `(argc, argv, envp, new_rsp)`
/// where `new_rsp` is the 16-byte aligned stack pointer to call `main` with.
unsafe fn write_args_envs_to_cage_stack(
    stack_top: usize,
    args: &[String],
    envs: &[(String, Option<String>)],
) -> (c_int, *const *const c_char, *const *const c_char, usize) {
    let mut cursor = stack_top;

    let mut write_str = |s: &str| -> *const c_char {
        let bytes = s.as_bytes();
        cursor -= bytes.len() + 1; // +1 for NUL terminator
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), cursor as *mut u8, bytes.len());
            *((cursor + bytes.len()) as *mut u8) = 0;
        }
        cursor as *const c_char
    };

    let arg_ptrs: Vec<*const c_char> = args.iter().map(|s| write_str(s)).collect();
    let env_strings: Vec<String> = envs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v.as_deref().unwrap_or("")))
        .collect();
    let env_ptrs: Vec<*const c_char> = env_strings.iter().map(|s| write_str(s)).collect();

    // Align down to pointer size before placing the argv/envp arrays.
    cursor &= !(std::mem::align_of::<*const c_char>() - 1);

    cursor -= (env_ptrs.len() + 1) * std::mem::size_of::<*const c_char>();
    let envp = cursor as *mut *const c_char;
    for (i, ptr) in env_ptrs.iter().enumerate() {
        unsafe { std::ptr::write(envp.add(i), *ptr) };
    }
    unsafe { std::ptr::write(envp.add(env_ptrs.len()), std::ptr::null()) };

    cursor -= (arg_ptrs.len() + 1) * std::mem::size_of::<*const c_char>();
    let argv = cursor as *mut *const c_char;
    for (i, ptr) in arg_ptrs.iter().enumerate() {
        unsafe { std::ptr::write(argv.add(i), *ptr) };
    }
    unsafe { std::ptr::write(argv.add(arg_ptrs.len()), std::ptr::null()) };

    // The SysV x86-64 ABI requires rsp to be 16-byte aligned at the `call` instruction.
    cursor &= !0xf;

    (
        args.len() as c_int,
        argv as *const *const c_char,
        envp as *const *const c_char,
        cursor,
    )
}

/// Switches to `new_rsp`, calls `main_fn`, then calls the isolated libc exit
/// function on that same stack. `new_rsp` must already be 16-byte aligned.
unsafe fn call_main_on_stack(
    new_rsp: usize,
    main_fn: MainFn,
    exit_fn: ExitFn,
    argc: c_int,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> ! {
    unsafe {
        asm!(
            "mov {new_rsp}, %rsp",
            "call *{func}",
            "mov %eax, %edi",
            "call *%r12",
            new_rsp = in(reg) new_rsp,
            func = in(reg) main_fn,
            in("r12") exit_fn, //callee saved!
            in("rdi") argc,
            in("rsi") argv,
            in("rdx") envp,
            clobber_abi("C"),
            options(att_syntax),
        );
    }
    std::hint::unreachable_unchecked()
}

unsafe fn write_process_entry_stack(
    stack_top: usize,
    args: &[String],
    envs: &[(String, Option<String>)],
    execfn: &str,
    auxv: &ProcessAuxv,
) -> usize {
    let mut cursor = stack_top;

    let mut write_str = |s: &str| -> *const c_char {
        let bytes = s.as_bytes();
        cursor -= bytes.len() + 1;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), cursor as *mut u8, bytes.len());
            *((cursor + bytes.len()) as *mut u8) = 0;
        }
        cursor as *const c_char
    };

    let arg_ptrs: Vec<*const c_char> = args.iter().map(|s| write_str(s)).collect();
    let env_strings: Vec<String> = envs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v.as_deref().unwrap_or("")))
        .collect();
    let env_ptrs: Vec<*const c_char> = env_strings.iter().map(|s| write_str(s)).collect();
    let execfn_ptr = write_str(execfn);
    let platform_ptr = write_str("x86_64");

    let mut random_bytes = [0u8; 16];
    let got = unsafe {
        libc::syscall(
            libc::SYS_getrandom,
            random_bytes.as_mut_ptr(),
            random_bytes.len(),
            0,
        )
    };
    if got != random_bytes.len() as libc::c_long {
        let seed = (current_tid() as u64) ^ (stack_top as u64) ^ (auxv.entry as u64);
        for (idx, byte) in random_bytes.iter_mut().enumerate() {
            let rotated = seed.rotate_left((idx as u32) * 5);
            *byte = (rotated as u8) ^ ((idx as u8).wrapping_mul(31));
        }
    }

    cursor -= random_bytes.len();
    unsafe {
        std::ptr::copy_nonoverlapping(random_bytes.as_ptr(), cursor as *mut u8, random_bytes.len());
    }
    let random_ptr = cursor as usize;

    cursor &= !(std::mem::align_of::<usize>() - 1);

    let aux_entries = [
        (auxv::AT_SYSINFO_EHDR, auxv.sysinfo_ehdr),
        (auxv::AT_MINSIGSTKSZ, auxv.minsigstksz),
        (auxv::AT_HWCAP, auxv.hwcap),
        (auxv::AT_PAGESZ, auxv.pagesz),
        (auxv::AT_CLKTCK, auxv.clktck),
        (auxv::AT_PHDR, auxv.phdr),
        (auxv::AT_PHENT, auxv.phent),
        (auxv::AT_PHNUM, auxv.phnum),
        (auxv::AT_BASE, auxv.base),
        (auxv::AT_FLAGS, auxv.flags),
        (auxv::AT_ENTRY, auxv.entry),
        (auxv::AT_UID, auxv.uid),
        (auxv::AT_EUID, auxv.euid),
        (auxv::AT_GID, auxv.gid),
        (auxv::AT_EGID, auxv.egid),
        (auxv::AT_SECURE, auxv.secure),
        (auxv::AT_RANDOM, random_ptr),
        (auxv::AT_HWCAP2, auxv.hwcap2),
        (auxv::AT_EXECFN, execfn_ptr as usize),
        (auxv::AT_PLATFORM, platform_ptr as usize),
        (auxv::AT_3ITRMP_PTR, auxv.threei_trampoline_ptr),
    ];

    // Keep the final entry RSP 16-byte aligned while preserving strict
    // argc/argv/envp/auxv adjacency expected by ELF process startup.
    let entry_region_words = 1
        + (arg_ptrs.len() + 1)
        + (env_ptrs.len() + 1)
        + (aux_entries.len() + 1) * 2;

    let entry_region_bytes = entry_region_words * std::mem::size_of::<usize>();
    let tentative_entry_rsp = cursor
        .checked_sub(entry_region_bytes)
        .expect("entry region underflow while building process stack");
    let align_fix = tentative_entry_rsp & 0xf;
    if align_fix != 0 {
        debug_assert_eq!(align_fix, std::mem::size_of::<usize>());
        cursor -= align_fix;
    }

    cursor -= (aux_entries.len() + 1) * 2 * std::mem::size_of::<usize>();
    let auxp = cursor as *mut usize;
    for (i, (kind, value)) in aux_entries.iter().enumerate() {
        unsafe {
            std::ptr::write(auxp.add(2 * i), *kind);
            std::ptr::write(auxp.add(2 * i + 1), *value);
        }
    }
    unsafe {
        std::ptr::write(auxp.add(2 * aux_entries.len()), auxv::AT_NULL);
        std::ptr::write(auxp.add(2 * aux_entries.len() + 1), 0);
    }

    cursor -= (env_ptrs.len() + 1) * std::mem::size_of::<usize>();
    let envp = cursor as *mut usize;
    for (i, ptr) in env_ptrs.iter().enumerate() {
        unsafe { std::ptr::write(envp.add(i), *ptr as usize) };
    }
    unsafe { std::ptr::write(envp.add(env_ptrs.len()), 0) };

    cursor -= (arg_ptrs.len() + 1) * std::mem::size_of::<usize>();
    let argv = cursor as *mut usize;
    for (i, ptr) in arg_ptrs.iter().enumerate() {
        unsafe { std::ptr::write(argv.add(i), *ptr as usize) };
    }
    unsafe { std::ptr::write(argv.add(arg_ptrs.len()), 0) };

    cursor -= std::mem::size_of::<usize>();
    unsafe { std::ptr::write(cursor as *mut usize, args.len()) };

    mpk_debug(format!(
        "entry stack layout: argc@{:#x} argv@{:#x} envp@{:#x} auxv@{:#x} rsp={:#x}",
        cursor,
        argv as usize,
        envp as usize,
        auxp as usize,
        cursor,
    ));
    debug_assert_eq!(cursor & 0xf, 0, "entry stack pointer must be 16-byte aligned");

    cursor
}

unsafe fn jump_to_entrypoint(new_rsp: usize, entrypoint: usize) -> ! {
    unsafe {
        asm!(
            "mov {new_rsp}, %rsp",
            "jmp *{entry}",
            new_rsp = in(reg) new_rsp,
            entry = in(reg) entrypoint,
            options(noreturn, att_syntax),
        );
    }
}

unsafe extern "C" fn noop_enable_interpose(
    _handler: Option<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64, i32, i64, u64) -> i64>,
    _make_threei_call: Option<extern "C" fn(
        u64, u64, u64, u64,
        u64, u64, u64, u64,
        u64, u64, u64, u64,
        u64, u64, u64, u64,
    ) -> i64>,
) -> c_int {
    0
}

// ── MPK SyscallRuntime implementation ────────────────────────────────────────

/// MPK runtime implementation.
pub struct MpkRuntime;

impl SyscallRuntime for MpkRuntime {
    fn handle_clone(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32 {
        mpk_clone_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        )
    }

    fn handle_exec(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32 {
        // arg1 = path, arg1_cageid = execing_cageid
        // arg2 = argv, arg2_cageid = envp (based on 3i convention)

        let _ = (cageid, arg4, arg4_cageid, arg5, arg5_cageid, arg6, arg6_cageid);
        
        let path_ptr = arg1 as *const c_char;
        let path_ptr_cageid = arg1_cageid;
        let argv_ptr = arg2 as *const *const c_char;
        let argv_ptr_cageid = arg2_cageid;
        let envp_ptr = arg3 as *const *const c_char;
        let envp_ptr_cageid = arg3_cageid;

        //TODO: handle argument gathering from different cages
        //cageid is the rawposix cageid
        match exec_mpk_internal(arg1_cageid, path_ptr, argv_ptr, envp_ptr) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("[lind-mpk] exec failed: {}", e);
                -1
            }
        }
    }

    fn handle_exit(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32 {
        mpk_exit_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        )
    }
}

// ── MPK syscall interposition and execution ──────────────────────────────────

/// Syscall interposition handler: forwards every native syscall issued inside
/// the isolated dlmopen namespace through 3i's dispatch table so it reaches
/// RawPOSIX for sandboxed handling, exactly like a Wasm cage.
///
/// This function is registered with the custom glibc via
/// `__enable_syscall_interpose`. Once registered, any libc-level syscall made
/// by the guest .so calls this handler instead of entering the kernel directly.
extern "C" fn lind_syscall_handler(
    a1: i64,
    a2: i64,
    a3: i64,
    a4: i64,
    a5: i64,
    a6: i64,
    _nargs: i32,
    number: i64,
    cage_id: u64
) -> i64 {
    let nargs = _nargs.clamp(0, 6) as usize;

    let arg1 = if nargs >= 1 { a1 as u64 } else { UNUSED_ARG };
    let arg2 = if nargs >= 2 { a2 as u64 } else { UNUSED_ARG };
    let arg3 = if nargs >= 3 { a3 as u64 } else { UNUSED_ARG };
    let arg4 = if nargs >= 4 { a4 as u64 } else { UNUSED_ARG };
    let arg5 = if nargs >= 5 { a5 as u64 } else { UNUSED_ARG };
    let arg6 = if nargs >= 6 { a6 as u64 } else { UNUSED_ARG };

    let a1_cid = cage_id;
    let a2_cid = cage_id;
    let retval = threei::make_syscall(
        cage_id as u64, // self_cageid
        number as u64,
        0,                    // _syscall_name: unused for native
        cage_id as u64, // target_cageid
        arg1,
        a1_cid,
        arg2,
        a2_cid,
        arg3,
        cage_id as u64,
        arg4,
        cage_id as u64,
        arg5,
        cage_id as u64,
        arg6,
        cage_id as u64,
    );

    crate::lind_mpk::signals::handle_signal(cage_id, None);
    retval
}


pub fn init_mpk(lind_manager: Arc<LindCageManager>) {
    mpk_debug("initializing lind-mpk");
    // Publish the manager globally so mpk_clone_syscall_entry can reach it.
    LIND_MANAGER.set(lind_manager).ok();

    crate::lind_mpk::signals::register_kill_syscall_handler()
        .expect("failed to register MPK kill syscall handler");
    crate::lind_mpk::signals::register_os_signal_handlers()
        .expect("failed to register MPK process signal handlers");

    threei::register_trampoline(
        threei_const::RUNTIME_TYPE_MPK,
        grate_callback_trampoline,
        0,
    );

    // Frees MpkThreadInfos queued by mpk_exit_syscall_entry once their OS
    // thread has actually exited (see RuntimeInfo::queue_thread_info_for_reap).
    std::thread::Builder::new()
        .name("mpk-thread-reaper".to_string())
        .spawn(crate::lind_mpk::RuntimeInfo::run_thread_info_reaper)
        .expect("failed to spawn mpk-thread-reaper thread");

    mpk_debug("lind-mpk initialized successfully");
}


/// Internal helper for MPK exec: tears down the old namespace and loads a new program.
///
/// This is called by MpkRuntime::handle_exec when the guest issues an exec syscall.
/// It performs the following steps:
/// 1. Retrieves and tears down the existing MPKRuntimeInfo (closes dlmopen handles)
/// 2. Parses the path, argv, and envp from the native pointers
/// 3. Loads and executes the new .so using the same logic as execute_mpk
fn exec_mpk_internal(
    cage_id: u64,
    path_ptr: *const c_char,
    argv_ptr: *const *const c_char,
    envp_ptr: *const *const c_char
) -> anyhow::Result<i32> {
    mpk_debug(format!("exec_mpk_internal for cage {}", cage_id));

    // Step 1: Parse all arguments into owned Rust values BEFORE any teardown.
    // The pointers live in the old namespace's memory; dlclose would invalidate them.

    let so_path = unsafe { CStr::from_ptr(path_ptr) }
        .to_str()
        .context("invalid UTF-8 in path")?
        .to_string();

    // step 1.1: locate the .so file and canonicalize it.
    let canonical_so_path = std::fs::canonicalize(&so_path)
        .context("failed to canonicalize .so path")?;
    let so_path = canonical_so_path
        .to_str()
        .context("invalid UTF-8 in .so path")?
        .to_owned();

    mpk_debug(format!("executing new program: {}", so_path));

    let mut args = Vec::new();
    if !argv_ptr.is_null() {
        let mut i = 0;
        loop {
            let arg_ptr = unsafe { *argv_ptr.offset(i) };
            if arg_ptr.is_null() {
                break;
            }
            let arg_str = unsafe { CStr::from_ptr(arg_ptr) }
                .to_str()
                .context("invalid UTF-8 in argv")?
                .to_string();
            args.push(arg_str);
            i += 1;
        }
    }

    let mut vars = Vec::new();
    if !envp_ptr.is_null() {
        let mut i = 0;
        loop {
            let env_ptr = unsafe { *envp_ptr.offset(i) };
            if env_ptr.is_null() {
                break;
            }
            let env_str = unsafe { CStr::from_ptr(env_ptr) }
                .to_str()
                .context("invalid UTF-8 in envp")?;

            // Split "KEY=VALUE" into (KEY, Some(VALUE))
            if let Some((key, val)) = env_str.split_once('=') {
                vars.push((key.to_string(), Some(val.to_string())));
            }
            i += 1;
        }
    }
    merge_host_ld_env_vars(&mut vars);
    ensure_ld_library_path_contains_lib_elf(&mut vars);

    mpk_debug(format!("parsed {} args, {} env vars", args.len(), vars.len()));

    // Step 2: Tear down the existing namespace context now that arguments are safely copied.
    let cage = get_cage(cage_id)
        .ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    let current_os_tid = current_tid();
    let old_grate_cage_ids = {
        let runtime_info = cage.runtime_info.read();
        let mpk_info = runtime_info
            .as_any()
            .downcast_ref::<MPKRuntimeInfo>()
            .ok_or_else(|| anyhow::anyhow!("cage {} does not have MPKRuntimeInfo; cannot exec", cage_id))?;
        mpk_info
            .threads
            .read()
            .get(&current_os_tid)
            .map(|thread| thread.thread_info.cage_ids.lock().unwrap().clone())
            .unwrap_or_default()
    };
    mpk_debug(format!(
        "exec: preserving registered grate cages for tid {}: {:?}",
        current_os_tid, old_grate_cage_ids
    ));
    {
        let runtime_info = cage.runtime_info.read();
        if let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() {
            if mpk_info.pid == 0 {
                mpk_debug("tearing down old namespace context");
                unsafe {
                    if !mpk_info.loader_libc_handle.is_null() {
                        libc::dlclose(mpk_info.loader_libc_handle);
                    }
                    if !mpk_info.loader_cage_handle.is_null() {
                        libc::dlclose(mpk_info.loader_cage_handle);
                    }
                }
                // Unmap the old cage memory only when running in the same process
                // (cage_pid == 0).  For forked children (cage_pid != 0) the memory
                // lives in the child's address space and is reclaimed when we kill it.
                if !mpk_info.memory_base.is_null() && mpk_info.memory_size > 0 {
                    mpk_debug("unmapping old cage memory region");
                    unsafe { libc::munmap(mpk_info.memory_base, mpk_info.memory_size); }
                }
                mpk_debug("old namespace context torn down");
            }
            else {
                mpk_debug("old namespace context belongs to a forked child; not tearing down");
            }
            
            let my_pid = unsafe { libc::getpid() };
            let cage_pid = mpk_info.pid;
            // If the cage has a non-zero PID and it's different from our own,
            // kill that process (it's a forked child)
            if cage_pid != 0 {
                assert!(
                    cage_pid != my_pid,
                    "mpk_exec_internal: Cannot kill self (cage_pid={}, my_pid={})",
                    cage_pid, my_pid
                );
                
                mpk_debug(format!("mpk_exec_internal: killing child process {}", cage_pid));
                unsafe {
                    libc::kill(cage_pid, libc::SIGKILL);
                }
            }
        }
        else {
            bail!("cage {} does not have MPKRuntimeInfo; cannot exec", cage_id);
        }
    }

    let enable_interpose: EnableInterposeF = noop_enable_interpose;
    ENABLE_INTERPOSE_PTR.store(noop_enable_interpose as usize as u64, Ordering::Release);

    // Step 5: Set up supervisor stack and install GS.
    // Returns MpkThreadInfo which is registered in MPKRuntimeInfo::threads below.
    let thread_info = Arc::new(match setup_supervisor_stack(cage_id, current_os_tid) {
        Ok(v) => v,
        Err(e) => return Err(e),
    });

    // Keep the new cage and all restored grates attached to the same
    // supervisor state for this OS thread.
    thread_info.register_cage(cage_id);
    for grate_cage_id in &old_grate_cage_ids {
        if *grate_cage_id != cage_id {
            thread_info.register_cage(*grate_cage_id);
        }
    }

    // Recreate this thread's grate stack in every cage that was registered
    // before exec replaced the current cage's MPK runtime.
    for grate_cage_id in &old_grate_cage_ids {
        if *grate_cage_id == cage_id {
            continue;
        }
        let grate = get_cage(*grate_cage_id)
            .ok_or_else(|| anyhow::anyhow!("grate cage {} not found during exec", grate_cage_id))?;
        let grate_runtime_info = grate.runtime_info.read();
        let grate_mpk_info = grate_runtime_info
            .as_any()
            .downcast_ref::<MPKRuntimeInfo>()
            .ok_or_else(|| anyhow::anyhow!("cage {} is not using MPK", grate_cage_id))?;
        let mut grate_threads = grate_mpk_info.threads.write();
        let mut grate_thread: MpkCageThreadInfo = MpkCageThreadInfo {
            thread_info: Arc::clone(&thread_info),
            grate_cage_id: *grate_cage_id,
            stack_addr: std::sync::atomic::AtomicUsize::new(0),
            stack_base: 0,
            stack_size: 0,
        };
        grate_thread.allocate_grate_stack()?;
        grate_threads.insert(current_os_tid, grate_thread);
        mpk_debug(format!(
            "exec: restored grate stack for cage {} and tid {}",
            grate_cage_id, current_os_tid
        ));
    }

    // Step 6: Map fresh 4 GB for the new program and initialize vmmap.
    mpk_debug("mapping 4 GB cage memory for new program with MAP_NORESERVE");
    let memory_base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            MPK_MEMORY_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
            -1,
            0,
        )
    };
    if memory_base == libc::MAP_FAILED {
        bail!("mmap for new cage memory failed: {}", std::io::Error::last_os_error());
    }
    mpk_debug(format!("new cage memory mapped at {memory_base:p}"));
    cage::init_vmmap(cage_id, memory_base as usize, None, VmmapBitWidth::Vmmap64Bit);

    // Step 7: Load custom ld.so + target binary into cage vmmap.
    let loaded = {
        let mut vmmap = cage.vmmap.write();
        match mpk_load_ldso_and_binary_with_info(DEFAULT_LDSO_PATH, &so_path, &mut vmmap) {
            Ok(v) => v,
            Err(e) => {
                unsafe { libc::munmap(memory_base, MPK_MEMORY_SIZE) };
                return Err(e);
            }
        }
    };
    let process_auxv = match build_process_auxv(
        &so_path,
        &loaded,
        lind_syscall_handler as *const () as usize,
    ) {
        Ok(v) => v,
        Err(e) => {
            unsafe { libc::munmap(memory_base, MPK_MEMORY_SIZE) };
            return Err(e);
        }
    };

    // Step 8: Update RuntimeInfo.
    let mpk_info = MPKRuntimeInfo::new(
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        enable_interpose,
        0,
        memory_base,
        MPK_MEMORY_SIZE,
        THREAD_START_ID + 1,
        Some((current_os_tid, MpkCageThreadInfo {
            thread_info: Arc::clone(&thread_info),
            grate_cage_id: cage_id,
            stack_addr: std::sync::atomic::AtomicUsize::new(0),
            stack_base: 0,
            stack_size: 0,
        })),
    );
    *cage.runtime_info.write() = Box::new(mpk_info);
    crate::lind_mpk::signals::register_mpk_memory_region(
        cage_id,
        memory_base as usize,
        MPK_MEMORY_SIZE,
    );
    cage.runtime_type.store(threei_const::RUNTIME_TYPE_MPK, Ordering::Release);
    mpk_debug(format!("updated MPKRuntimeInfo for cage {} (main tid={})", cage_id, current_os_tid));

    // Note: register_mpk_handler_for_cage does not need to be called.

    // Step 9: Allocate a stack for the guest inside the cage's own vmmap,
    // write argc/argv/envp entry layout, then jump into ld.so.
    let (_, cage_stack_top) = allocate_stack_in_vmmap(cage_id, CAGE_STACK_GUARD, CAGE_STACK_SIZE)?;
    let entry_rsp = unsafe {
        write_process_entry_stack(cage_stack_top, &args, &vars, &so_path, &process_auxv)
    };

    mpk_debug(format!(
        "jumping to ld.so entrypoint {:#x} on cage stack rsp={:#x} (binary_entry={:#x})",
        loaded.ldso_entrypoint,
        entry_rsp,
        loaded.binary_entrypoint,
    ));
    unsafe { jump_to_entrypoint(entry_rsp, loaded.ldso_entrypoint as usize) }
}

pub fn execute_mpk(lindboot_cli: CliOptions, cage_id: u64) -> anyhow::Result<i32> {
    let so_path = lindboot_cli.wasm_file();

    // Propagate the --no-interpose flag globally before any interpose call.
    NO_INTERPOSE.store(lindboot_cli.no_interpose, Ordering::Release);
    if lindboot_cli.no_interpose {
        mpk_debug("--no-interpose: syscall interposition disabled");
    }

    mpk_debug(format!("starting execute_mpk for {}, cwd={}", so_path, std::env::current_dir().unwrap().display()));
    
    // step 0: locate the .so file and canonicalize it.
    let canonical_so_path = std::fs::canonicalize(&so_path)
        .context("failed to canonicalize .so path")?;
    let so_path = canonical_so_path
        .to_str()
        .context("invalid UTF-8 in .so path")?
        .to_owned();
    let enable_interpose: EnableInterposeF = noop_enable_interpose;
    ENABLE_INTERPOSE_PTR.store(noop_enable_interpose as usize as u64, Ordering::Release);

    // Step 1: Allocate the supervisor stack and set GS.
    // Returns MpkThreadInfo which is registered in MPKRuntimeInfo::threads below.
    let thread_info = match setup_supervisor_stack(cage_id, current_tid()) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };

    // Step 2: Map 4 GB for the cage's virtual address space with MAP_NORESERVE,
    // then load ld.so + binary into vmmap and store MPKRuntimeInfo.
    mpk_debug("mapping 4 GB cage memory with MAP_NORESERVE");
    let memory_base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            MPK_MEMORY_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
            -1,
            0,
        )
    };
    if memory_base == libc::MAP_FAILED {
        bail!("mmap failed: {}", std::io::Error::last_os_error());
    }
    mpk_debug(format!("cage memory mapped at {memory_base:p}"));

    // Get the cage, initialize its vmmap, and store MPKRuntimeInfo.
    let cage = get_cage(cage_id)
        .ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    cage::init_vmmap(cage_id, memory_base as usize, None, VmmapBitWidth::Vmmap64Bit);
    let loaded = {
        let mut vmmap = cage.vmmap.write();
        match mpk_load_ldso_and_binary_with_info(DEFAULT_LDSO_PATH, &so_path, &mut vmmap) {
            Ok(v) => v,
            Err(e) => {
                unsafe { libc::munmap(memory_base, MPK_MEMORY_SIZE) };
                return Err(e);
            }
        }
    };
    let process_auxv = match build_process_auxv(
        &so_path,
        &loaded,
        lind_syscall_handler as *const () as usize,
    ) {
        Ok(v) => v,
        Err(e) => {
            unsafe { libc::munmap(memory_base, MPK_MEMORY_SIZE) };
            return Err(e);
        }
    };

    let tid = current_tid();
    thread_info.register_cage(cage_id);
    let mpk_info = MPKRuntimeInfo::new(
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        enable_interpose,
        0,
        memory_base,
        MPK_MEMORY_SIZE,
        THREAD_START_ID + 1,
        Some((tid, MpkCageThreadInfo {
            thread_info: Arc::new(thread_info),
            grate_cage_id: cage_id,
            stack_addr: std::sync::atomic::AtomicUsize::new(0),
            stack_base: 0,
            stack_size: 0,
        })),
    );
    *cage.runtime_info.write() = Box::new(mpk_info);
    crate::lind_mpk::signals::register_mpk_memory_region(
        cage_id,
        memory_base as usize,
        MPK_MEMORY_SIZE,
    );
    cage.runtime_type.store(threei_const::RUNTIME_TYPE_MPK, Ordering::Release);
    mpk_debug(format!("MPKRuntimeInfo stored in cage {} (main tid={})", cage_id, tid));

    // Step 3: Notify threei of the cage runtime type
    // (syscall handler registration is now done once at boot by shims::register_syscall_entries)
    threei::set_cage_runtime(cage_id, threei_const::RUNTIME_TYPE_MPK);
    register_mpk_handler_for_cage(cage_id)?;


    // Step 4: initialize signalling

    // MPK has no Wasmtime epoch; pass a pointer to a static zero so
    // lind_signal_init stores a valid (disabled) epoch handler address.
    let epoch_pointer: *mut u64 = MPK_EPOCH.as_ptr();  
    lind_signal_init(
        cage_id,
        epoch_pointer,
        THREAD_START_ID, //this is the first thread of the new cage
        true, /* this is the main thread */
    );


    // Step 5: Allocate a stack for the guest inside the cage's own vmmap and
    // write argc/argv/envp entry layout.
    let (_, cage_stack_top) = allocate_stack_in_vmmap(cage_id, CAGE_STACK_GUARD, CAGE_STACK_SIZE)?;
    let mut launch_vars = lindboot_cli.vars.clone();
    merge_host_ld_env_vars(&mut launch_vars);
    ensure_ld_library_path_contains_lib_elf(&mut launch_vars);

    let entry_rsp = unsafe {
        write_process_entry_stack(
            cage_stack_top,
            &lindboot_cli.args,
            &launch_vars,
            &so_path,
            &process_auxv,
        )
    };

    mpk_debug(format!(
        "jumping to ld.so entrypoint {:#x} on cage stack rsp={:#x} (binary_entry={:#x})",
        loaded.ldso_entrypoint,
        entry_rsp,
        loaded.binary_entrypoint,
    ));

    // Step 6: Transfer control to the loader entrypoint; execution returns via guest exit paths.
    unsafe { jump_to_entrypoint(entry_rsp, loaded.ldso_entrypoint as usize) }
}