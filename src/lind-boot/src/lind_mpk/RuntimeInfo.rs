use libc::{c_void, pid_t};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use cage::{get_cage, MemoryBackingType, RuntimeInfo, RwLock, VmmapOps};
use sysdefs::constants::fs_const::{PAGESHIFT, PROT_NONE, PROT_READ, PROT_WRITE};

const MPK_GRATE_STACK_SIZE: usize = 8 * 1024 * 1024;
const MPK_GRATE_STACK_GUARD: usize = 4096;

fn align_up_to_pages(value: usize) -> usize {
    let page_size = 1usize << PAGESHIFT;
    (value + page_size - 1) & !(page_size - 1)
}

/// Allocates a guard-page + usable stack region out of `cage_id`'s own vmmap.
/// The cage's backing memory is already reserved via its 4 GB MAP_NORESERVE
/// region, so only the vmmap bookkeeping and the guard-page protection need
/// to be set up here. Returns `(guard_start_sys_addr, stack_top_sys_addr)`.
pub fn allocate_stack_in_vmmap(
    cage_id: u64,
    guard_size: usize,
    usable_size: usize,
) -> anyhow::Result<(usize, usize)> {
    let allocation_size = guard_size + usable_size;
    let npages = allocation_size >> PAGESHIFT;
    let guard_pages = guard_size >> PAGESHIFT;

    let cage = get_cage(cage_id).ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    let mut vmmap = cage.vmmap.write();

    let space = vmmap
        .find_map_space(npages, 1)
        .ok_or_else(|| anyhow::anyhow!("no space in cage {} vmmap for stack", cage_id))?;

    let guard_start = space.start();
    let stack_start = guard_start + guard_pages;

    // Guard page: reserved in the vmmap, no access.
    vmmap.add_entry_with_overwrite(
        guard_start,
        guard_pages,
        PROT_NONE,
        PROT_NONE,
        0,
        MemoryBackingType::Anonymous,
        0,
        0,
        cage_id,
    )?;

    // Usable stack region.
    vmmap.add_entry_with_overwrite(
        stack_start,
        npages - guard_pages,
        PROT_READ | PROT_WRITE,
        PROT_READ | PROT_WRITE,
        0,
        MemoryBackingType::Anonymous,
        0,
        0,
        cage_id,
    )?;

    let guard_start_sys = vmmap.page_num_to_sys(guard_start);
    drop(vmmap);

    if unsafe { libc::mprotect(guard_start_sys as *mut c_void, guard_size, libc::PROT_NONE) } != 0 {
        anyhow::bail!(
            "mprotect for stack guard failed: {}",
            std::io::Error::last_os_error()
        );
    }

    Ok((guard_start_sys, guard_start_sys + allocation_size))
}

/// Releases a stack region previously allocated by `allocate_stack_in_vmmap`,
/// removing its vmmap bookkeeping and restoring the guard page to read/write.
pub fn free_stack_in_vmmap(cage_id: u64, guard_start_sys: usize, allocation_size: usize, guard_size: usize) {
    if let Some(cage) = get_cage(cage_id) {
        let mut vmmap = cage.vmmap.write();
        let start_page = vmmap.sys_to_page_num(guard_start_sys);
        let npages = allocation_size >> PAGESHIFT;
        let _ = vmmap.remove_entry(start_page, npages);
    }
    unsafe {
        libc::mprotect(
            guard_start_sys as *mut c_void,
            guard_size,
            libc::PROT_READ | libc::PROT_WRITE,
        );
    }
}

/// Allocates a writable region out of `cage_id`'s vmmap and returns
/// `(region_base_sys_addr, region_size)`.
pub(super) fn allocate_region_in_vmmap(cage_id: u64, size: usize) -> anyhow::Result<(usize, usize)> {
    if size == 0 {
        anyhow::bail!("cannot allocate zero-sized vmmap region");
    }

    let allocation_size = align_up_to_pages(size);
    let npages = allocation_size >> PAGESHIFT;
    let cage = get_cage(cage_id).ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    let mut vmmap = cage.vmmap.write();

    let space = vmmap
        .find_map_space(npages, 1)
        .ok_or_else(|| anyhow::anyhow!("no space in cage {} vmmap for region", cage_id))?;
    let start_page = space.start();
    vmmap.add_entry_with_overwrite(
        start_page,
        npages,
        PROT_READ | PROT_WRITE,
        PROT_READ | PROT_WRITE,
        0,
        MemoryBackingType::Anonymous,
        0,
        0,
        cage_id,
    )?;
    let region_base = vmmap.page_num_to_sys(start_page);
    drop(vmmap);

    unsafe {
        std::ptr::write_bytes(region_base as *mut u8, 0, allocation_size);
    }

    Ok((region_base, allocation_size))
}

/// Releases a region previously allocated by `allocate_region_in_vmmap`.
fn free_region_in_vmmap(cage_id: u64, base_sys: usize, allocation_size: usize) {
    if base_sys == 0 || allocation_size == 0 {
        return;
    }
    if let Some(cage) = get_cage(cage_id) {
        let mut vmmap = cage.vmmap.write();
        let start_page = vmmap.sys_to_page_num(base_sys);
        let npages = allocation_size >> PAGESHIFT;
        let _ = vmmap.remove_entry(start_page, npages);
    }
}

/// Type alias for the __enable_syscall_interpose function pointer.
/// This function is provided by the custom glibc loaded in the dlmopen
/// namespace and is used to register syscall interposition handlers.
pub type EnableInterposeF = unsafe extern "C" fn(
    handler: Option<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64, i32, i64, u64) -> i64>,
    make_threei_call: Option<extern "C" fn(
        u64, u64, u64, u64, 
        u64, u64, u64, u64, 
        u64, u64, u64, u64, 
        u64, u64, u64, u64, 
    ) -> i64>,
) -> libc::c_int;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LindAllocateTlsResult {
    pub stackaddr: *mut c_void,
    pub fs_base: *mut c_void,
    pub stacktop: *mut c_void,
    pub dtv: *mut c_void,
    pub dtv_size: usize,
}

//int _lind_dl_allocate_tls (struct lind_dl_tls_result *result)
pub type DlAllocateTlsF = unsafe extern "C" fn(result: *mut LindAllocateTlsResult) -> libc::c_int;

/// Maximum number of nested MPK contexts in a thread's GS segment.
pub const LIND_MPK_MAX_CONTEXTS: usize = 16;

/// Fourth word of a saved context.
///
/// User contexts may store a PKRU value. A supervisor context entered from a guest
/// trampoline stores the address of the supervisor-stack slot that holds the
/// caller's context-stack pointer. Clone rewrites those slots for the child.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types)]
pub union PKRU_ContextStackPtr {
    pub pkru: u64,
    pub context_stack_ptr: *mut *mut MPKSupervisorCtxStack,
}

impl Default for PKRU_ContextStackPtr {
    fn default() -> Self {
        Self { pkru: 0 }
    }
}

impl std::fmt::Debug for PKRU_ContextStackPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", unsafe { self.pkru })
    }
}

/// A saved MPK execution context.
/// NOTE: The field offsets are used in assembly. Changes here need to be reflected in the corresponding assembly code.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MPKSupervisorContext {
    pub rsp: u64,
    pub fs_base: usize,
    pub cage_id: u64, // 0 for supervisor
    /// For supervisor contexts entered from the guest: pointer to the slot on the
    /// supervisor stack holding the caller's context-stack pointer. Needed for the
    /// clone return path.
    pub pkru_context_stack_ptr: PKRU_ContextStackPtr,
}

const _: () = assert!(std::mem::size_of::<MPKSupervisorContext>() == 32);
const _: () = assert!(std::mem::offset_of!(MPKSupervisorContext, pkru_context_stack_ptr) == 24);


/// Context stack shared with the guest syscall interposition assembly.
///
/// The GS base points at this struct:
///   - gs:0  — current context index
///   - gs:8  — OS thread ID
///   - gs:16 — fixed-capacity saved-context array
///
/// Supervisor Rust code accesses this struct through `current_ctx_stack` in host
/// TLS. Guest glibc reaches it through FS, on the page after its TCB, where the
/// runtime aliases it (see `alias_context_stack`).
/// GS base is retained for loader TLS setup and cloned-frame recovery.
/// Each thread must have its own instance because its active context is written on
/// every intercepted syscall.
#[repr(C)]
#[derive(Debug)]
pub struct MPKSupervisorCtxStack {
    pub current_context: usize,
    pub os_tid: u64,
    pub contexts: [MPKSupervisorContext; LIND_MPK_MAX_CONTEXTS],
}

thread_local! {
    #[allow(non_upper_case_globals)]
    pub(super) static current_ctx_stack: std::cell::Cell<*mut MPKSupervisorCtxStack> =
        const { std::cell::Cell::new(std::ptr::null_mut()) };
}

pub(super) fn current_context_stack() -> *mut MPKSupervisorCtxStack {
    let stack = current_ctx_stack.get();
    assert!(!stack.is_null(), "MPK context stack is not initialized");
    stack
}

// ── Guest view of the context stack ─────────────────────────────────────────
//
// Guest glibc locates the context stack on the first page after its TCB,
// `(fs + tcb_size + PAGE - 1) & !(PAGE - 1)`. Glibc leaves that page blank
// (MAP_LIND_TLS mappings and grate stacks reserve it). Whenever a thread
// pointer is installed for a thread (arch_prctl(ARCH_SET_FS), clone, grate TLS
// setup), the supervisor-owned context-stack page is aliased onto it with
// mremap(old_size = 0), so supervisor updates are visible through every alias.

pub(super) const CONTEXT_PAGE_SIZE: usize = 1usize << PAGESHIFT;

/// `sizeof(struct pthread)` of the guest glibc, read from `_lind_tls_tcb_size`.
/// Read directly by the supervisor-side trampolines.
pub(super) static GUEST_TCB_SIZE: AtomicUsize = AtomicUsize::new(0);

pub fn set_guest_tcb_size(size: usize) -> anyhow::Result<()> {
    if size == 0 || size > CONTEXT_PAGE_SIZE {
        anyhow::bail!("unsupported guest TCB size {size:#x}");
    }
    match GUEST_TCB_SIZE.compare_exchange(0, size, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Ok(()),
        Err(existing) if existing == size => Ok(()),
        Err(existing) => anyhow::bail!(
            "guest TCB size changed from {existing:#x} to {size:#x}; all cages must use the same ld.so"
        ),
    }
}

pub fn guest_tcb_size() -> anyhow::Result<usize> {
    match GUEST_TCB_SIZE.load(Ordering::Acquire) {
        0 => anyhow::bail!("guest TCB size is not known; ld.so has not been loaded"),
        size => Ok(size),
    }
}

/// Page at which guest glibc expects the context stack for thread pointer `fs`.
pub fn context_stack_page_for_fs(fs: usize, tcb_size: usize) -> Option<usize> {
    fs.checked_add(tcb_size + CONTEXT_PAGE_SIZE - 1)
        .map(|end| end & !(CONTEXT_PAGE_SIZE - 1))
}

/// Allocates a thread's context pages: a private bootstrap page followed by
/// the shareable context-stack page. Until glibc installs its own TLS, the
/// guest FS points into the bootstrap page (see `bootstrap_fs_base`).
/// Returns `(mapping_base, mapping_size, context_stack)`.
pub(super) fn allocate_context_pages() -> anyhow::Result<(usize, usize, *mut MPKSupervisorCtxStack)> {
    let size = 2 * CONTEXT_PAGE_SIZE;
    let base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if base == libc::MAP_FAILED {
        anyhow::bail!("mmap for MPK context pages failed: {}", std::io::Error::last_os_error());
    }
    let stack_page = base as usize + CONTEXT_PAGE_SIZE;
    // The context-stack page must be a shared mapping so mremap can alias it.
    // TODO: the context-stack page must be tagged with the supervisor MPK key.
    let shared = unsafe {
        libc::mmap(
            stack_page as *mut c_void,
            CONTEXT_PAGE_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_ANONYMOUS | libc::MAP_FIXED,
            -1,
            0,
        )
    };
    if shared == libc::MAP_FAILED {
        let error = std::io::Error::last_os_error();
        unsafe { libc::munmap(base, size) };
        anyhow::bail!("mmap for MPK context-stack page failed: {error}");
    }
    Ok((base as usize, size, stack_page as *mut MPKSupervisorCtxStack))
}

/// Guest FS base used before glibc installs its TLS: the TCB ends exactly at
/// the context-stack page allocated by `allocate_context_pages`.
pub fn bootstrap_fs_base(context_stack: *mut MPKSupervisorCtxStack) -> anyhow::Result<usize> {
    Ok(context_stack as usize - guest_tcb_size()?)
}

fn alias_page(source_page: usize, target_page: usize) -> std::io::Result<()> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_mremap,
            source_page,
            0usize,
            CONTEXT_PAGE_SIZE,
            libc::MREMAP_MAYMOVE | libc::MREMAP_FIXED,
            target_page,
        )
    };
    if result == -1 {
        return Err(std::io::Error::last_os_error());
    }
    if result as usize != target_page {
        return Err(std::io::Error::other("mremap did not honour MREMAP_FIXED"));
    }
    Ok(())
}

/// Current guest alias of each context stack, per cage, keyed by the
/// supervisor address of the context stack. Needed to give a forked child a
/// private context stack.
static CONTEXT_STACK_ALIASES: OnceLock<Mutex<HashMap<usize, HashMap<u64, usize>>>> = OnceLock::new();

fn context_stack_aliases_map() -> &'static Mutex<HashMap<usize, HashMap<u64, usize>>> {
    CONTEXT_STACK_ALIASES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Guest addresses currently aliasing `context_stack`.
pub fn context_stack_aliases(context_stack: *mut MPKSupervisorCtxStack) -> Vec<usize> {
    context_stack_aliases_map()
        .lock()
        .unwrap()
        .get(&(context_stack as usize))
        .map(|aliases| aliases.values().copied().collect())
        .unwrap_or_default()
}

fn forget_context_stack_aliases(context_stack: *mut MPKSupervisorCtxStack) {
    context_stack_aliases_map().lock().unwrap().remove(&(context_stack as usize));
}

/// Maps `context_stack` onto the page guest glibc derives from thread pointer
/// `fs` in cage `cage_id`. The target page must lie inside the cage memory
/// `[region_base, region_base + region_size)`, since `fs` is guest-controlled.
pub fn alias_context_stack(
    context_stack: *mut MPKSupervisorCtxStack,
    cage_id: u64,
    fs: usize,
    region_base: usize,
    region_size: usize,
) -> anyhow::Result<()> {
    let target = map_context_stack_alias(
        context_stack,
        fs,
        guest_tcb_size()?,
        region_base,
        region_size,
    )?;
    context_stack_aliases_map()
        .lock()
        .unwrap()
        .entry(context_stack as usize)
        .or_default()
        .insert(cage_id, target);
    Ok(())
}

fn map_context_stack_alias(
    context_stack: *mut MPKSupervisorCtxStack,
    fs: usize,
    tcb_size: usize,
    region_base: usize,
    region_size: usize,
) -> anyhow::Result<usize> {
    let target = context_stack_page_for_fs(fs, tcb_size)
        .ok_or_else(|| anyhow::anyhow!("guest FS {fs:#x} overflows"))?;
    let region_end = region_base.saturating_add(region_size);
    if region_base == 0
        || target < region_base
        || target.checked_add(CONTEXT_PAGE_SIZE).is_none_or(|end| end > region_end)
    {
        anyhow::bail!(
            "context-stack page {target:#x} for guest FS {fs:#x} is outside cage memory {region_base:#x}..{region_end:#x}"
        );
    }
    alias_page(context_stack as usize, target)
        .map_err(|error| anyhow::anyhow!("aliasing context stack to {target:#x} failed: {error}"))?;
    Ok(target)
}

/// After fork, the child shares the context-stack page with the parent.
/// Gives the child a private copy initialised from `contents` (snapshotted
/// before fork) and re-points `aliases` at it.
pub unsafe fn privatize_context_stack_after_fork(
    context_stack: *mut MPKSupervisorCtxStack,
    contents: &MPKSupervisorCtxStack,
    aliases: &[usize],
) -> anyhow::Result<()> {
    let page = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            CONTEXT_PAGE_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if page == libc::MAP_FAILED {
        anyhow::bail!("mmap for private context stack failed: {}", std::io::Error::last_os_error());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(contents, page as *mut MPKSupervisorCtxStack, 1);
    }
    let result = std::iter::once(context_stack as usize)
        .chain(aliases.iter().copied())
        .try_for_each(|target| {
            alias_page(page as usize, target).map_err(|error| {
                anyhow::anyhow!("re-aliasing context stack at {target:#x} after fork failed: {error}")
            })
        });
    unsafe { libc::munmap(page, CONTEXT_PAGE_SIZE) };
    result
}

#[cfg(test)]
mod context_stack_tests {
    use super::*;

    #[test]
    fn current_context_stack_is_thread_local() {
        let mut stack = MPKSupervisorCtxStack {
            current_context: 0,
            os_tid: 0,
            contexts: [MPKSupervisorContext::default(); LIND_MPK_MAX_CONTEXTS],
        };
        let previous = current_ctx_stack.replace(&mut stack);
        assert_eq!(current_context_stack(), &mut stack as *mut _);
        std::thread::spawn(|| {
            assert!(current_ctx_stack.get().is_null());
            let mut child_stack = MPKSupervisorCtxStack {
                current_context: 0,
                os_tid: 0,
                contexts: [MPKSupervisorContext::default(); LIND_MPK_MAX_CONTEXTS],
            };
            current_ctx_stack.set(&mut child_stack);
            assert_eq!(current_context_stack(), &mut child_stack as *mut _);
            current_ctx_stack.set(std::ptr::null_mut());
        })
        .join()
        .unwrap();
        assert_eq!(current_context_stack(), &mut stack as *mut _);
        current_ctx_stack.set(previous);
    }

    const PAGE: usize = CONTEXT_PAGE_SIZE;
    const TCB: usize = 0x900;

    fn map_region(pages: usize) -> usize {
        let region = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                pages * PAGE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert_ne!(region, libc::MAP_FAILED);
        region as usize
    }

    #[test]
    fn context_stack_page_is_first_page_after_tcb() {
        assert_eq!(context_stack_page_for_fs(0x10000 - TCB, TCB), Some(0x10000));
        assert_eq!(context_stack_page_for_fs(0x10000 - TCB - 0x40, TCB), Some(0x10000));
        assert_eq!(context_stack_page_for_fs(0x10000 - TCB + 1, TCB), Some(0x11000));
        assert_eq!(context_stack_page_for_fs(usize::MAX - 0x10, TCB), None);
    }

    #[test]
    fn context_pages_place_bootstrap_tcb_before_context_stack() {
        let (base, size, stack) = allocate_context_pages().unwrap();
        assert_eq!(size, 2 * PAGE);
        assert_eq!(stack as usize, base + PAGE);
        assert_eq!(context_stack_page_for_fs(stack as usize - TCB, TCB), Some(stack as usize));
        unsafe { libc::munmap(base as *mut c_void, size) };
    }

    #[test]
    fn context_stack_alias_shares_memory_and_is_bounds_checked() {
        let (base, size, stack) = allocate_context_pages().unwrap();
        let region = map_region(4);
        let fs = region + 2 * PAGE - TCB;

        assert!(map_context_stack_alias(stack, fs, TCB, region, PAGE * 2).is_err());
        assert!(map_context_stack_alias(stack, region - TCB, TCB, region + PAGE, PAGE).is_err());
        assert!(map_context_stack_alias(stack, fs, TCB, 0, usize::MAX).is_err());

        let alias = map_context_stack_alias(stack, fs, TCB, region, 4 * PAGE).unwrap();
        assert_eq!(alias, region + 2 * PAGE);
        unsafe {
            (*stack).os_tid = 0x1234;
            assert_eq!((*(alias as *mut MPKSupervisorCtxStack)).os_tid, 0x1234);
            (*(alias as *mut MPKSupervisorCtxStack)).current_context = 3;
            assert_eq!((*stack).current_context, 3);
            libc::munmap(region as *mut c_void, 4 * PAGE);
            libc::munmap(base as *mut c_void, size);
        }
    }

    #[test]
    fn fork_child_gets_private_context_stack() {
        let (base, size, stack) = allocate_context_pages().unwrap();
        let region = map_region(2);
        let alias = map_context_stack_alias(stack, region + PAGE - TCB, TCB, region, 2 * PAGE).unwrap();
        unsafe { (*stack).os_tid = 1 };

        let snapshot = unsafe { std::ptr::read(stack) };
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            let ok = unsafe { privatize_context_stack_after_fork(stack, &snapshot, &[alias]) }.is_ok()
                && unsafe { (*stack).os_tid } == 1;
            unsafe {
                (*stack).os_tid = 2;
                let shared = (*(alias as *const MPKSupervisorCtxStack)).os_tid == 2;
                libc::_exit(if ok && shared { 0 } else { 1 });
            }
        }
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0) };
        assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0);
        unsafe {
            assert_eq!((*stack).os_tid, 1, "child write leaked into the parent");
            assert_eq!((*(alias as *const MPKSupervisorCtxStack)).os_tid, 1);
            libc::munmap(region as *mut c_void, 2 * PAGE);
            libc::munmap(base as *mut c_void, size);
        }
    }
}


/// Per-thread supervisor stack state for MPK syscall interposition.
///
/// Each in-process thread needs its own `MPKSupervisorCtxStack` and supervisor stack.
/// Instances are stored in `MPKRuntimeInfo::threads`, keyed by OS thread ID
/// (`gettid()`), and are freed when removed from that map or when the cage exits.
#[derive(Debug)]
pub struct MpkThreadInfo {
    /// GS segment data for this thread, stored in the mapped context page and
    /// installed via arch_prctl(ARCH_SET_GS, ...).
    /// The GS data is stable accross all cages for this thread and only accessible to 
    /// the supervisor.
    pub gs_data: *mut MPKSupervisorCtxStack,
    /// Mapping containing the bootstrap TCB page followed by the GS segment
    /// data (see `allocate_context_pages`).
    pub context_pages: usize,
    pub context_pages_size: usize,
    /// Base address of the mmap'd supervisor stack region (includes guard page).
    /// Stored as `usize` rather than a raw pointer so that `MpkThreadInfo` is `Send`.
    pub supervisor_stack_base: usize,
    /// Total allocation size of the supervisor stack region in bytes (guard + usable).
    pub supervisor_stack_size: usize,
    /// Cage IDs whose thread maps contain an MpkCageThreadInfo for this OS thread.
    pub cage_ids: Mutex<HashSet<u64>>,
    /// Host-thread stack pointer captured near thread start, used as a safe stack
    /// to run exit cleanup without touching the supervisor stack allocation.
    pub exit_stack: AtomicUsize,
    /// Pointer to child_tid in guest memory to atomically clear and futex wake on thread exit.
    pub child_tid: AtomicU64,
}

impl MpkThreadInfo {
    pub fn register_cage(&self, cage_id: u64) {
        self.cage_ids.lock().unwrap().insert(cage_id);
    }

    /// Aliases this thread's context stack behind the guest TCB at `fs` in
    /// cage `cage_id` whose memory is `[region_base, region_base + region_size)`.
    pub fn alias_context_stack(
        &self,
        cage_id: u64,
        fs: usize,
        region_base: usize,
        region_size: usize,
    ) -> anyhow::Result<()> {
        alias_context_stack(self.gs_data, cage_id, fs, region_base, region_size)
    }
}


impl Drop for MpkThreadInfo {
    fn drop(&mut self) {
        unsafe {
            if !self.gs_data.is_null() {
                forget_context_stack_aliases(self.gs_data);
                std::ptr::drop_in_place(self.gs_data);
            }
        }
        if self.context_pages != 0 && self.context_pages_size != 0 {
            unsafe {
                libc::munmap(self.context_pages as *mut c_void, self.context_pages_size);
            }
        }
        if self.supervisor_stack_base != 0 && self.supervisor_stack_size != 0 {
            unsafe {
                libc::munmap(
                    self.supervisor_stack_base as *mut c_void,
                    self.supervisor_stack_size,
                );
            }
        }
        // The context page is unmapped after dropping its initialized GS data.
    }
}

// SAFETY: MpkThreadInfo is owned by MPKRuntimeInfo::threads, which is protected
// by a RwLock. Its pointers refer to mapped context/stack regions and are not
// accessed through Rust references concurrently.
unsafe impl Send for MpkThreadInfo {}
unsafe impl Sync for MpkThreadInfo {}

// ── Deferred cleanup for still-running supervisor stacks ────────────────────
//
// A cage's exit handler runs on its own OS thread's supervisor stack. If that
// call drops the last `Arc<MpkThreadInfo>` synchronously, `MpkThreadInfo::drop`
// munmaps the very stack the code is executing on. Instead, the last reference
// is queued here and only dropped by `run_thread_info_reaper` once the OS
// thread has actually terminated.
static PENDING_THREAD_REAP: OnceLock<Mutex<Vec<(pid_t, Arc<MpkThreadInfo>)>>> = OnceLock::new();

fn pending_thread_reap() -> &'static Mutex<Vec<(pid_t, Arc<MpkThreadInfo>)>> {
    PENDING_THREAD_REAP.get_or_init(|| Mutex::new(Vec::new()))
}

/// Queues `thread_info` to be dropped once OS thread `tid` has exited.
pub fn queue_thread_info_for_reap(tid: pid_t, thread_info: Arc<MpkThreadInfo>) {
    pending_thread_reap().lock().unwrap().push((tid, thread_info));
}

/// Returns whether OS thread `tid` is still alive in this process.
fn os_thread_is_alive(tid: pid_t) -> bool {
    let ret = unsafe { libc::syscall(libc::SYS_tgkill, libc::getpid(), tid, 0) };
    ret == 0
}

/// Periodically drops queued `MpkThreadInfo`s whose OS thread has exited,
/// until `stop_thread_info_reaper` is called. Spawned once from `init_mpk`.
pub fn run_thread_info_reaper() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        pending_thread_reap()
            .lock()
            .unwrap()
            .retain(|(tid, _)| os_thread_is_alive(*tid));
    }
}


#[derive(Debug)]
pub struct MpkCageThreadInfo {
    pub thread_info: Arc<MpkThreadInfo>,
    /// The grate whose vmmap backs this thread's stack.
    pub grate_cage_id: u64,
    ///This is the stack the thread will use inside the grate.
    pub stack_addr: AtomicUsize,
    pub stack_base: usize,
    pub stack_size: usize,
    pub fs_base: usize,
    pub dtv: *mut c_void,
    pub dtv_size: usize,
}

impl MpkCageThreadInfo {
    const DTV_BUFFER_SIZE: usize = 4 * 1024;

    /// Allocates this thread's grate stack out of the grate's own vmmap
    /// (`grate_cage_id`), rather than an independent mmap. The grate's
    /// backing memory is already reserved via its 4 GB MAP_NORESERVE region,
    /// so only the vmmap bookkeeping and the guard-page protection need to
    /// be set up here.
    pub fn allocate_grate_stack(&mut self) -> anyhow::Result<()> {
        if self.stack_addr.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }

        // One extra page above the stack top is left blank for the context-stack
        // alias that guest glibc expects right after the TCB (see
        // `initialize_grate_tls`).
        let (stack_base, allocation_end) = allocate_stack_in_vmmap(
            self.grate_cage_id,
            MPK_GRATE_STACK_GUARD,
            MPK_GRATE_STACK_SIZE + CONTEXT_PAGE_SIZE,
        )?;

        self.stack_base = stack_base;
        self.stack_size = MPK_GRATE_STACK_GUARD + MPK_GRATE_STACK_SIZE + CONTEXT_PAGE_SIZE;
        self.stack_addr.store(allocation_end - CONTEXT_PAGE_SIZE, Ordering::Relaxed);
        Ok(())
    }

    /// Allocates the grate TLS below the stack top with `_lind_dl_allocate_tls`
    /// and aliases this thread's context stack onto the reserved page after
    /// the new TCB. `grate` is the runtime info of `self.grate_cage_id`.
    pub fn initialize_grate_tls(&mut self, grate: &MPKRuntimeInfo) -> anyhow::Result<()> {
        let stack_addr = self.stack_addr.load(Ordering::Relaxed);
        if stack_addr == 0 {
            anyhow::bail!("grate stack is not initialized");
        }
        if self.dtv.is_null() {
            let (dtv_base, dtv_size) = allocate_region_in_vmmap(self.grate_cage_id, Self::DTV_BUFFER_SIZE)?;
            self.dtv = dtv_base as *mut c_void;
            self.dtv_size = dtv_size;
        }

        let mut tls_result = LindAllocateTlsResult {
            stackaddr: stack_addr as *mut c_void,
            fs_base: std::ptr::null_mut(),
            stacktop: std::ptr::null_mut(),
            dtv: self.dtv,
            dtv_size: self.dtv_size,
        };
        let ret = unsafe { (grate.dl_allocate_tls)(&mut tls_result as *mut LindAllocateTlsResult) };
        if ret != 0 {
            anyhow::bail!(
                "_lind_dl_allocate_tls failed: {}",
                std::io::Error::last_os_error()
            );
        }
        if tls_result.fs_base.is_null() || tls_result.stacktop.is_null() {
            anyhow::bail!("_lind_dl_allocate_tls returned null fs_base or stacktop");
        }

        let fs_base = tls_result.fs_base as usize;
        let expected_alias = stack_addr;
        if context_stack_page_for_fs(fs_base, guest_tcb_size()?) != Some(expected_alias) {
            anyhow::bail!(
                "grate TCB at {fs_base:#x} does not end below the reserved context-stack page {expected_alias:#x}"
            );
        }
        self.thread_info.alias_context_stack(
            self.grate_cage_id,
            fs_base,
            grate.memory_base as usize,
            grate.memory_size,
        )?;

        self.fs_base = fs_base;
        self.stack_addr
            .store(tls_result.stacktop as usize, Ordering::Relaxed);
        Ok(())
    }
}

impl Drop for MpkCageThreadInfo {
    fn drop(&mut self) {
        // The stack lives inside the grate's vmmap/backing memory, which is
        // owned and torn down by the grate itself; only release the vmmap
        // bookkeeping and restore the guard page's protection here.
        if self.stack_base != 0 && self.stack_size != 0 {
            free_stack_in_vmmap(
                self.grate_cage_id,
                self.stack_base,
                self.stack_size,
                MPK_GRATE_STACK_GUARD,
            );
        }
        if !self.dtv.is_null() {
            free_region_in_vmmap(self.grate_cage_id, self.dtv as usize, self.dtv_size);
            self.dtv = std::ptr::null_mut();
            self.dtv_size = 0;
        }
    }
}

/// Runtime-specific information for MPK (Memory Protection Keys) based cages.
///
/// This structure stores the dlmopen handles and function pointers needed
/// to manage isolated native .so execution. Fields are set once during
/// execute_mpk initialization and then only read (e.g., during fork/clone).
/// The Cage's RwLock<Box<dyn RuntimeInfo>> provides synchronization.
#[derive(Debug)]
pub struct MPKRuntimeInfo {
    /// Handle to the guest .so loaded via dlmopen
    pub loader_cage_handle: *mut c_void,
    /// Handle to the custom libc loaded in the isolated namespace
    pub loader_libc_handle: *mut c_void,
    /// Function pointer to __enable_syscall_interpose in custom libc
    pub enable_interpose_fn: EnableInterposeF,
    /// Function pointer to _lind_dl_allocate_tls in custom libc, needed for thread-local storage allocation.
    pub dl_allocate_tls: DlAllocateTlsF,
    /// OS-level process ID of this cage's process.
    /// 0 if the cage runs in the main lind process; non-zero for forked children.
    pub pid: libc::pid_t,
    /// Base address of the 4 GB MAP_NORESERVE region backing this cage's vmmap.
    /// Null if no mapping has been established yet.
    pub memory_base: *mut c_void,
    /// Size (in bytes) of the memory region at memory_base.
    pub memory_size: usize,
    /// Next thread ID to assign for threads created in this cage.
    pub next_thread_id: RwLock<i32>,
    /// Per-thread supervisor stack state, keyed by OS thread ID (gettid()).
    /// Each in-process thread that runs guest code registers its MpkThreadInfo here
    /// so its supervisor stack and GS context data are tracked and freed on exit.
    /// Empty for forked child cages (their resources live in their own address space).
    pub threads: RwLock<HashMap<pid_t, MpkCageThreadInfo>>,
    /// MPK cages do not have a Wasmtime epoch, so lifecycle transitions are
    /// kept explicitly and observed at MPK entry points.
    pub stopped: AtomicBool,
    pub killed: AtomicBool,
    pub state_lock: Mutex<()>,
    pub state_cv: Condvar,
}

impl MPKRuntimeInfo {
    /// Creates a new MPKRuntimeInfo.
    ///
    /// `initial_thread` is the `MpkThreadInfo` for the thread calling this constructor.
    /// Pass `Some(thread_info)` for in-process cages (execute_mpk / exec_mpk_internal);
    /// pass `None` for the parent-side record of a forked child cage (the child's thread
    /// resources live in the child's address space and are managed there).
    pub fn new(
        cage_handle: *mut c_void,
        libc_handle: *mut c_void,
        enable_interpose: EnableInterposeF,
        dl_allocate_tls: DlAllocateTlsF,
        pid: libc::pid_t,
        memory_base: *mut c_void,
        memory_size: usize,
        next_thread_id: i32,
        initial_thread: Option<(pid_t, MpkCageThreadInfo)>,
    ) -> Self {
        let mut thread_map = HashMap::new();
        if let Some((tid, info)) = initial_thread {
            thread_map.insert(tid, info);
        }
        MPKRuntimeInfo {
            loader_cage_handle: cage_handle,
            loader_libc_handle: libc_handle,
            enable_interpose_fn: enable_interpose,
            dl_allocate_tls,
            pid,
            memory_base,
            memory_size,
            next_thread_id: RwLock::new(next_thread_id),
            threads: RwLock::new(thread_map),
            stopped: AtomicBool::new(false),
            killed: AtomicBool::new(false),
            state_lock: Mutex::new(()),
            state_cv: Condvar::new(),
        }
    }
}

impl RuntimeInfo for MPKRuntimeInfo {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Safety: Raw pointers in MPKRuntimeInfo point to dlmopen handles and mmap'd regions
// that remain valid for the cage's lifetime. Access is synchronized through the Cage's
// RwLock<Box<dyn RuntimeInfo>>, ensuring no data races. The handles are opaque library
// objects and the function pointer is extern "C" and statically determined, making them
// safe to share. The threads map is protected by its own RwLock.
unsafe impl Send for MPKRuntimeInfo {}
unsafe impl Sync for MPKRuntimeInfo {}